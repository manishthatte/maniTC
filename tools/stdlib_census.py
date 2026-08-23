#!/usr/bin/env python3
"""Census the ManiT standard library, by measurement, and regenerate its reference.

docs/stdlib-reference.md was hand-written on 8 April 2026 and never revisited.
By 21 August it listed roughly a third of the surface, omitted every function
added since, and — the part that matters — said nothing about which entries
actually work. That is not a documentation problem, it is the declared-vs-defined
problem in prose form: `fmt::` once documented 25 functions that were declared in
ManiT, declared in LLVM, and defined nowhere (ORACLE_FINDINGS.md Section 14b).
A reference that lists a name it has not called is a claim, not a record.

So this script does two things and writes down both:

  1. Parses `stdlib/*.mt` for every declaration — name, parameters, return type,
     the doc comment above it, and whether it has a ManiT body or is `// native`.
     The sources are the only honest origin for a signature.

  2. CALLS each one, on both backends. It writes a one-line program per
     function, compiles it to LLVM and to T3ISA, and records which of the two
     accept it. A `// native` declaration with no definition fails at link on
     LLVM and at assembly on T3, which is precisely the failure the old
     reference could not see.

Functions whose parameters this script cannot synthesise a value for — a `Vec`,
a `Map`, a struct — are marked "not probed" rather than assumed working. An
unprobed entry is an admission, not a pass.

Usage:
    python3 tools/stdlib_census.py            # rewrite docs/stdlib-reference.md
    python3 tools/stdlib_census.py --check    # exit 1 if the doc is out of date
    python3 tools/stdlib_census.py --json     # dump the raw census

Author: Manish Jagdish Thatte
"""

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
from concurrent.futures import ThreadPoolExecutor

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
STDLIB = os.path.join(ROOT, "stdlib")
MANITC = os.environ.get("MANITC") or os.path.join(ROOT, "target", "release", "manitc")
DOC = os.path.join(ROOT, "docs", "stdlib-reference.md")

BEGIN = "<!-- BEGIN GENERATED: stdlib_census.py -->"
END = "<!-- END GENERATED: stdlib_census.py -->"

# Modules that carry free functions. `collections` and `sync` declare none —
# they are types with methods, resolved in semantic/analyzer/type_inference.rs
# rather than in stdlib source — so their tables stay hand-written above.
MODULE_ORDER = [
    "io", "fmt", "str", "math", "ternary", "bridge", "crypto",
    "t27f", "time", "env", "fs", "net", "test",
]

# `async` and `collections` and `sync` declare no free functions — their surface
# is methods on Future/Task/Vec/Map/Set/Deque/TernaryTrie, resolved in
# semantic/analyzer/type_inference.rs rather than in stdlib source. Their
# sections stay hand-written above the generated block.

MODULE_BLURB = {
    "io":      "Console and stream I/O. Every function here is native, which is why a third of them are missing on one backend or the other — a native is two implementations, and nothing checked that both were written.",
    "fmt":     "Rendering values as text. Four functions are native — `format`, `show_int`, "
               "`show_float`, `show_bool` — and the rest are ManiT written over them, so both "
               "backends get one body.",
    "str":     "String inspection and manipulation. The ten primitives are native; the other "
               "forty are ManiT over those ten.",
    "math":    "Arithmetic. Every float function is implemented in ManiT — T3 is ternary "
               "hardware with no libm, so a native would have meant writing the function twice. "
               "Three balanced-ternary helpers stay native because the T3 emulator answers them "
               "directly.",
    "ternary": "The balanced-ternary core. Mixed: the primitives the backends lower directly "
               "stay native, and everything built on top of them is ManiT.",
    "bridge":  "Binary/ternary interop — packing trits into bytes and back.",
    "crypto":  "Ternary cipher, hash and TRNG primitives.",
    "t27f":    "The 27-trit floating-point format, implemented in ManiT over `word`.",
    "time":    "Clocks and sleeping.",
    # No count in this blurb, deliberately. It used to open "Twenty-five of the
    # twenty-six do not work on at least one backend", and on 23 August 2026
    # that became twenty-two without the sentence changing — the whole point of
    # generating this file is that the Works column cannot go stale, and a
    # frozen number in the prose beside it can.
    "env":     "Process environment, arguments, and exit. Largely one-sided: most entries are "
               "present on LLVM and absent from the T3 emulator, so a program using them "
               "compiles for one target and fails to assemble for the other. The argument trio "
               "— `argc`, `arg`, `args` — is the exception and works on both. Read the Works "
               "column before relying on an entry.",
    "fs":      "File system access. Like `env::`, largely one-sided — see the Works column before relying on an entry.",
    "net":     "Sockets. Two separate things are true here. On LLVM this runtime is built "
               "with `-DMANIT_NO_GUI`, which compiles network support out and raises a "
               "deliberate diagnostic — a gate, not a missing symbol. On T3 there is no "
               "definition at all.",
    "test":    "Assertions. Pure ManiT over `io::println` and `env::exit`, so it needs nothing "
               "from the C runtime and works identically on both backends. The condition is "
               "`bool3`, not `bool`: `tand`, `tor` and comparisons against `unknown` produce a "
               "genuine three-valued answer, and an assertion that cannot tell `false` from "
               "`unknown` reports a verdict it does not have. Hence three entry points — "
               "`assert`, `assert_unknown`, `assert_false` — one per trit.",
}

# A value of each type that a probe can pass. Types absent here cannot be
# synthesised, and their functions are reported as not probed.
SAMPLE = {
    "int": "0", "word": "0", "float": "0.0", "str": '"x"', "char": "'a'",
    "bool": "true", "bool3": "unknown", "trit": "0",
    "t9": "0", "t27": "0", "t54": "0", "tryte": "0", "trint": "0",
    "tfloat": "0.0",
}

FN_RE = re.compile(
    r"^(?P<pub>pub\s+)?fn\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)\s*"
    r"\((?P<params>[^)]*)\)\s*"
    r"(?:->\s*(?P<ret>[^;{]+?))?\s*"
    r"(?P<tail>;|\{)"
)


def parse_module(path):
    """Return [{name, params, ret, doc, native}] for one stdlib source."""
    out, doc = [], []
    with open(path) as fh:
        lines = fh.readlines()
    for i, raw in enumerate(lines):
        line = raw.rstrip("\n")
        stripped = line.strip()
        if not stripped:
            # A blank line ends a comment block. Without this the module's
            # header — "Usage: use std::io; io::println(...)" — accumulated into
            # the first function's description.
            doc = []
            continue
        if stripped.startswith("//"):
            text = stripped[2:].strip()
            if not text or set(text) <= set("-=# "):
                # A banner rule or an empty comment line is a separator, not
                # documentation, and it ends the block.
                doc = []
            else:
                doc.append(text)
            continue
        m = FN_RE.match(line)
        if not m:
            doc = []
            continue
        params = []
        for p in m.group("params").split(","):
            p = p.strip()
            if not p:
                continue
            if ":" in p:
                pname, pty = p.split(":", 1)
                params.append((pname.strip(), pty.strip()))
            else:
                params.append((p, "?"))
        out.append({
            "name": m.group("name"),
            "params": params,
            "ret": (m.group("ret") or "").strip(),
            "doc": " ".join(doc).strip(),
            "native": m.group("tail") == ";",
            "line": i + 1,
        })
        doc = []
    return out


def signature(fn):
    ps = ", ".join(f"{n}: {t}" for n, t in fn["params"])
    sig = f"({ps})"
    if fn["ret"]:
        sig += f" -> {fn['ret']}"
    return sig


def probe_source(module, fn):
    """A program that calls `fn`, or None if its parameters cannot be built."""
    args = []
    for _, ty in fn["params"]:
        if ty not in SAMPLE:
            return None
        args.append(SAMPLE[ty])
    call = f"{module}::{fn['name']}({', '.join(args)})"
    if fn["ret"] and fn["ret"] not in ("", "void"):
        # Bind the result so the call cannot be treated as a bare statement and
        # discarded before it reaches codegen.
        body = f"    let _r = {call};\n    io::println(\"ok\");\n"
    else:
        body = f"    {call};\n    io::println(\"ok\");\n"
    return f"use std::{module};\nfn main() {{\n{body}}}\n"


def probe(module, fn, workdir):
    """Compile a call to `fn` for both backends. Returns (llvm_ok, t3_ok) or None."""
    src = probe_source(module, fn)
    if src is None:
        return None
    stem = os.path.join(workdir, f"{module}_{fn['name']}")
    path = stem + ".mt"
    with open(path, "w") as fh:
        fh.write(src)
    env = dict(os.environ, MANIT_NO_GUI="1")
    results = []
    for target, out in (("llvm", stem + ".bin"), ("t3", stem + ".t3b")):
        r = subprocess.run(
            [MANITC, "compile", "--target", target, "-o", out, path],
            capture_output=True, text=True, env=env,
        )
        if r.returncode == 0:
            results.append("ok")
        elif "runtime was built without network support" in (r.stdout + r.stderr):
            # A deliberate guard with its own diagnostic — the symbol exists,
            # this build just excluded it. Not the same thing as absent.
            results.append("gated")
        else:
            results.append("missing")
    return tuple(results)


def census():
    mods = {}
    with tempfile.TemporaryDirectory(prefix="manit_census_") as workdir:
        for module in MODULE_ORDER:
            path = os.path.join(STDLIB, module + ".mt")
            if not os.path.exists(path):
                continue
            fns = parse_module(path)
            with ThreadPoolExecutor(max_workers=os.cpu_count() or 4) as pool:
                verdicts = list(pool.map(lambda f: probe(module, f, workdir), fns))
            for fn, v in zip(fns, verdicts):
                fn["probe"] = v
            mods[module] = fns
    return mods


def availability(fn):
    v = fn.get("probe")
    if v is None:
        return "not probed"
    llvm, t3 = v
    if llvm == "ok" and t3 == "ok":
        return "yes"
    if llvm == "gated":
        return "needs net build" if t3 == "ok" else "**T3 missing**, needs net build"
    if llvm == "ok":
        return "**LLVM only**"
    if t3 == "ok":
        return "**T3 only**"
    return "**neither**"


def render(mods):
    out = [BEGIN, ""]
    total = ok = unprobed = broken = gated = 0
    for module in MODULE_ORDER:
        fns = mods.get(module)
        if not fns:
            continue
        out.append(f"## std::{module}")
        out.append("")
        if module in MODULE_BLURB:
            out.append(MODULE_BLURB[module])
            out.append("")
        out.append("| Function | Signature | Body | Works | Description |")
        out.append("|---|---|---|---|---|")
        for fn in sorted(fns, key=lambda f: f["name"]):
            total += 1
            avail = availability(fn)
            if avail == "yes":
                ok += 1
            elif avail == "not probed":
                unprobed += 1
            elif avail == "needs net build":
                gated += 1
            else:
                broken += 1
            body = "native" if fn["native"] else "ManiT"
            desc = fn["doc"].replace("|", "\\|") or "—"
            out.append(
                f"| `{module}::{fn['name']}` | `{signature(fn).replace('|', chr(92) + '|')}` "
                f"| {body} | {avail} | {desc} |"
            )
        out.append("")
    out.append("---")
    out.append("")
    out.append("## Census")
    out.append("")
    out.append(
        f"**{total} declarations** across {len([m for m in MODULE_ORDER if mods.get(m)])} "
        f"modules: **{ok} call cleanly on both backends**; {unprobed} were not probed "
        f"(their parameters are types this census cannot synthesise — a `Vec`, a `Map`, "
        f"a struct); and **{broken} do not work on at least one backend**."
    )
    out.append("")
    out.append(
        "The *Works* column records whether a one-line program calling the function "
        "compiles and links on each backend. It is a test of existence, not of "
        "correctness: a function marked `yes` has a body on both sides, which is exactly "
        "what `fmt::`'s twenty-five documented-but-undefined entries did not. "
        "A `not probed` row is an admission, not a pass."
    )
    out.append("")
    out.append(END)
    return "\n".join(out)


def splice(doc_text, generated):
    if BEGIN in doc_text and END in doc_text:
        head = doc_text[: doc_text.index(BEGIN)]
        tail = doc_text[doc_text.index(END) + len(END):]
        return head + generated + tail
    return doc_text.rstrip() + "\n\n" + generated + "\n"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true",
                    help="exit 1 if docs/stdlib-reference.md is out of date")
    ap.add_argument("--json", action="store_true", help="dump the raw census")
    ap.add_argument("--manitc", help="compiler to probe with (default: target/release/manitc, "
                                     "or $MANITC)")
    args = ap.parse_args()

    if args.manitc:
        global MANITC
        MANITC = args.manitc

    if not os.path.exists(MANITC):
        sys.exit(f"{MANITC} not found — run `cargo build --release` first")

    mods = census()
    if args.json:
        print(json.dumps(mods, indent=2))
        return

    generated = render(mods)
    with open(DOC) as fh:
        current = fh.read()
    updated = splice(current, generated)

    if args.check:
        if updated != current:
            sys.exit("docs/stdlib-reference.md is out of date — "
                     "run `python3 tools/stdlib_census.py`")
        print("docs/stdlib-reference.md is up to date")
        return

    with open(DOC, "w") as fh:
        fh.write(updated)
    print(f"wrote {DOC}")


if __name__ == "__main__":
    main()
