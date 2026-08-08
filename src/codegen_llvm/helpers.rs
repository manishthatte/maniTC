// codegen_llvm/helpers.rs — free helper functions for LLVM IR generation.
// Used by LLVMEmitter (mod.rs). Not part of the emitter struct.

use crate::ir::*;
use std::collections::HashMap;

pub(crate) fn llvm_type(ty: &IRType) -> String {
    match ty {
        IRType::I64 => "i64".to_string(),
        IRType::F64 => "double".to_string(),
        IRType::I8 => "i8".to_string(),
        IRType::I16 => "i16".to_string(),
        IRType::I32 => "i32".to_string(),
        IRType::Bool => "i1".to_string(),
        IRType::Trit => "i8".to_string(), // stored as -1, 0, +1
        IRType::Void => "void".to_string(),
        IRType::Ptr(_inner) => "ptr".to_string(), // opaque pointer, LLVM 15+
        IRType::Array(elem, n) => format!("[{} x {}]", n, llvm_type(elem)),
        IRType::Struct(name) => format!("%struct.{}", name),
    }
}

/// Natural alignment in bytes (as a decimal string literal).
pub(crate) fn llvm_align(ty: &IRType) -> &'static str {
    match ty {
        IRType::I64 | IRType::F64 => "8",
        IRType::I32 => "4",
        IRType::I16 => "2",
        IRType::I8 | IRType::Trit => "1",
        IRType::Bool => "1",
        IRType::Ptr(_) => "8",
        _ => "8",
    }
}

/// Zero initialiser string for a type (used when no explicit init is given).
pub(crate) fn llvm_zero_init(ty: &IRType) -> String {
    match ty {
        IRType::F64 => "0.0".to_string(),
        IRType::Bool => "false".to_string(),
        IRType::Ptr(_) => "null".to_string(),
        IRType::Array(_, _) | IRType::Struct(_) => "zeroinitializer".to_string(),
        IRType::Void => "zeroinitializer".to_string(),
        _ => "0".to_string(),
    }
}

// ---------------------------------------------------------------------------
// IRConst → LLVM operand string
// ---------------------------------------------------------------------------

pub(crate) fn irconst_to_string(c: &IRConst, hint_ty: &IRType) -> String {
    match c {
        IRConst::Int(n) => format!("{}", n),
        IRConst::Float(f) => {
            // Emit as LLVM hex double (0x...) to preserve exact bit pattern.
            format!("0x{:016X}", f.to_bits())
        }
        IRConst::Bool(b) => {
            if *b { "true".to_string() } else { "false".to_string() }
        }
        IRConst::Trit(t) => format!("{}", t),
        IRConst::Str(label) => {
            // The operand for a string literal is the pointer to the global.
            format!("@{}", label.trim_start_matches('@'))
        }
        IRConst::Null => match hint_ty {
            IRType::Ptr(_) => "null".to_string(),
            IRType::F64 => "0.0".to_string(),
            IRType::Bool => "false".to_string(),
            _ => "0".to_string(),
        },
    }
}

/// Convert an IRValue to an LLVM operand string without an emitter context
/// (used only for global-variable initialisers).
pub(crate) fn irvalue_to_operand(val: &IRValue, ty: &IRType) -> String {
    match val {
        IRValue::Temp(t) => format!("%{}", t.0),
        IRValue::Const(c) => irconst_to_string(c, ty),
        IRValue::Global(name) => format!("@{}", name),
        IRValue::Void => llvm_zero_init(ty),
    }
}

// ---------------------------------------------------------------------------
// Integer width helper (for coercion decisions)
// ---------------------------------------------------------------------------

pub(crate) fn int_width(ty_str: &str) -> u32 {
    match ty_str {
        "i1" => 1,
        "i8" => 8,
        "i16" => 16,
        "i32" => 32,
        "i64" => 64,
        _ => 64,
    }
}

// ---------------------------------------------------------------------------
// Type-inference helpers
// ---------------------------------------------------------------------------

/// Best-effort type of a value (used where the explicit type is unavailable,
/// e.g. call argument lists or return-value terminators).
pub(crate) fn guess_value_type(val: &IRValue) -> IRType {
    match val {
        IRValue::Const(IRConst::Int(_)) => IRType::I64,
        IRValue::Const(IRConst::Float(_)) => IRType::F64,
        IRValue::Const(IRConst::Bool(_)) => IRType::Bool,
        IRValue::Const(IRConst::Trit(_)) => IRType::I8,
        IRValue::Const(IRConst::Str(_)) => IRType::Ptr(Box::new(IRType::I8)),
        IRValue::Const(IRConst::Null) => IRType::Ptr(Box::new(IRType::I8)),
        IRValue::Global(_) => IRType::Ptr(Box::new(IRType::I8)),
        IRValue::Temp(_) => IRType::I64, // conservative fallback
        IRValue::Void => IRType::Void,
    }
}

/// For comparison BinOps the *result* type is Bool (i1) but the *operand*
/// type must be the original numeric type.  This function recovers that type.
pub(crate) fn operand_type_for_cmp(lhs: &IRValue, rhs: &IRValue, declared_ty: &IRType) -> IRType {
    // If the declared result is already non-Bool, use it directly.
    if !matches!(declared_ty, IRType::Bool) {
        return declared_ty.clone();
    }
    // Try to recover from lhs / rhs constants.
    let lhs_ty = guess_value_type(lhs);
    if !matches!(lhs_ty, IRType::Bool) {
        return lhs_ty;
    }
    let rhs_ty = guess_value_type(rhs);
    if !matches!(rhs_ty, IRType::Bool) {
        return rhs_ty;
    }
    // Both operands look like booleans — comparison is between i1 values.
    IRType::Bool
}

// ---------------------------------------------------------------------------
// Cast instruction selection
// ---------------------------------------------------------------------------

pub(crate) fn pick_cast_op(from: &IRType, to: &IRType) -> &'static str {
    match (from, to) {
        // Sign-extending widening
        (IRType::I8, IRType::I16)
        | (IRType::I8, IRType::I32)
        | (IRType::I8, IRType::I64)
        | (IRType::I16, IRType::I32)
        | (IRType::I16, IRType::I64)
        | (IRType::I32, IRType::I64)
        | (IRType::Trit, IRType::I16)
        | (IRType::Trit, IRType::I32)
        | (IRType::Trit, IRType::I64) => "sext",

        // Bool (i1) → integer: zero-extend (true=1, false=0)
        (IRType::Bool, IRType::I8)
        | (IRType::Bool, IRType::I16)
        | (IRType::Bool, IRType::I32)
        | (IRType::Bool, IRType::I64) => "zext",

        // Truncation / narrowing
        (IRType::I64, IRType::I32)
        | (IRType::I64, IRType::I16)
        | (IRType::I64, IRType::I8)
        | (IRType::I32, IRType::I16)
        | (IRType::I32, IRType::I8)
        | (IRType::I16, IRType::I8)
        | (IRType::I64, IRType::Bool)
        | (IRType::I32, IRType::Bool)
        | (IRType::I16, IRType::Bool)
        | (IRType::I8, IRType::Bool) => "trunc",

        // Integer → float
        (IRType::I64, IRType::F64)
        | (IRType::I32, IRType::F64)
        | (IRType::I16, IRType::F64)
        | (IRType::I8, IRType::F64) => "sitofp",

        // Float → integer
        (IRType::F64, IRType::I64)
        | (IRType::F64, IRType::I32)
        | (IRType::F64, IRType::I16)
        | (IRType::F64, IRType::I8) => "fptosi",

        // Pointer ↔ integer
        (IRType::Ptr(_), IRType::I64) | (IRType::Ptr(_), IRType::I32) => "ptrtoint",
        (IRType::I64, IRType::Ptr(_)) | (IRType::I32, IRType::Ptr(_)) => "inttoptr",

        // Pointer ↔ pointer (same under opaque ptrs, but bitcast is still valid)
        (IRType::Ptr(_), IRType::Ptr(_)) => "bitcast",

        // Trit narrowing (Trit widening already handled above)
        (IRType::I64, IRType::Trit) | (IRType::I32, IRType::Trit) => "trunc",

        // Default: bitcast (catches any remaining same-size reinterpretations)
        _ => "bitcast",
    }
}

// ---------------------------------------------------------------------------
// Function name mangling
// ---------------------------------------------------------------------------

/// Mangle a maniT function name to a valid LLVM identifier.
/// Replaces `::`, `<`, `>`, `,` and spaces with `_`.
pub(crate) fn mangle_func_name(name: &str) -> String {
    name.replace("::", "_")
        .replace('<', "_")
        .replace('>', "_")
        .replace(',', "_")
        .replace(' ', "_")
}

// ---------------------------------------------------------------------------
// Declare-line parser for building the function signature map
// ---------------------------------------------------------------------------

/// Parse LLVM `declare` lines to extract function signatures.
/// Returns a map from function name to (param_types, return_type).
pub(crate) fn parse_declare_sigs(decl_text: &str) -> HashMap<String, (Vec<String>, String)> {
    let mut sigs = HashMap::new();
    for line in decl_text.lines() {
        let line = line.trim();
        if !line.starts_with("declare ") {
            continue;
        }
        // Format: "declare <ret_type> @<name>(<params>)"
        let rest = &line[8..]; // skip "declare "
        // Find @name
        let at_pos = match rest.find('@') {
            Some(p) => p,
            None => continue,
        };
        let ret_str = rest[..at_pos].trim().to_string();
        let after_at = &rest[at_pos + 1..];
        // Find opening paren
        let paren_pos = match after_at.find('(') {
            Some(p) => p,
            None => continue,
        };
        let name = after_at[..paren_pos].to_string();
        // Extract params between ( and )
        let close_paren = match after_at.find(')') {
            Some(p) => p,
            None => continue,
        };
        let params_str = &after_at[paren_pos + 1..close_paren];
        let params: Vec<String> = if params_str.trim().is_empty() {
            vec![]
        } else {
            params_str
                .split(',')
                .map(|p| p.trim().to_string())
                .filter(|p| p != "...")
                .collect()
        };
        sigs.insert(name, (params, ret_str));
    }
    sigs
}

// ---------------------------------------------------------------------------
// stdlib declare statements (maniT runtime)
// ---------------------------------------------------------------------------

pub(crate) const STDLIB_DECLARES: &str = "\
; ---- io ----
declare void @io_println(ptr)
declare void @io_print(ptr)
declare void @io_newline()
declare void @io_print_int(i64)
declare void @io_println_int(i64)
declare void @io_print_float(double)
declare void @io_println_float(double)
declare void @io_print_char(i8)
declare void @io_print_trit(i8)
declare void @io_print_bool3(i8)
declare void @io_print_tryte(i8)
declare ptr @io_read_line()
declare i64 @io_read_int()

; ---- fmt ----
declare ptr @fmt_format(ptr, ...)
declare ptr @fmt_concat(ptr, ptr)
declare ptr @fmt_int_to_str(i64)
declare ptr @fmt_show_int(i64)
declare ptr @fmt_show_float(double)
declare ptr @fmt_show_bool(i1)
declare ptr @fmt_show_trit(i8)
declare ptr @fmt_show_bool3(i8)
declare ptr @fmt_pad_zeros(ptr, i64)
declare ptr @fmt_align_left(ptr, i64)
declare ptr @fmt_align_right(ptr, i64)
declare ptr @fmt_to_upper(ptr)
declare ptr @fmt_to_lower(ptr)

; ---- math ----
declare i64 @math_abs(i64)
declare double @math_abs_float(double)
declare double @math_sqrt(double)
declare double @math_pow(double, double)
declare double @math_log(double)
declare double @math_log2(double)
declare double @math_log3(double)
declare double @math_floor(double)
declare double @math_ceil(double)
declare double @math_round(double)
declare i64 @math_min(i64, i64)
declare i64 @math_max(i64, i64)
declare i64 @math_clamp(i64, i64, i64)
declare double @math_sin(double)
declare double @math_cos(double)
declare i64 @math_trit_count(i64)

; ---- str ----
declare i64 @str_len(ptr)
declare i8 @str_char_at(ptr, i64)
declare ptr @str_concat(ptr, ptr)
declare ptr @str_substr(ptr, i64, i64)
declare i1 @str_starts_with(ptr, ptr)
declare i1 @str_ends_with(ptr, ptr)
declare i1 @str_contains(ptr, ptr)
declare i64 @str_find(ptr, ptr)
declare ptr @str_replace(ptr, ptr, ptr)
declare ptr @str_split(ptr, ptr)
declare ptr @str_trim(ptr)
declare i64 @str_parse_int(ptr)
declare double @str_parse_float(ptr)
declare ptr @str_from_int(i64)
declare ptr @str_from_char(i64)
declare ptr @str_to_upper(ptr)
declare ptr @str_to_lower(ptr)
declare i1 @str_eq(ptr, ptr)

; ---- Vec<T> ----
declare ptr @Vec_new()
declare void @Vec_push(ptr, i64)
declare i64 @Vec_pop(ptr)
declare i64 @Vec_get(ptr, i64)
declare void @Vec_set(ptr, i64, i64)
declare i64 @Vec_len(ptr)
declare i1 @Vec_is_empty(ptr)
declare void @Vec_clear(ptr)
declare i1 @Vec_contains(ptr, i64)
declare void @Vec_remove(ptr, i64)
declare void @Vec_sort(ptr)
declare void @Vec_reverse(ptr)
declare i64 @Vec_index_of(ptr, i64)
declare void @Vec_for_each(ptr, ptr)
declare ptr @Vec_map(ptr, ptr)
declare ptr @Vec_filter(ptr, ptr)
declare i64 @Vec_fold(ptr, i64, ptr)
declare ptr @Vec_slice(ptr, i64, i64)

; ---- Map<K,V> ----
declare ptr @Map_new()
declare void @Map_insert(ptr, i64, i64)
declare i64 @Map_get(ptr, i64)
declare i64 @Map_get_or(ptr, i64, i64)
declare i1 @Map_contains_key(ptr, i64)
declare void @Map_remove(ptr, i64)
declare i64 @Map_len(ptr)
declare i1 @Map_is_empty(ptr)
declare ptr @Map_keys(ptr)
declare ptr @Map_values(ptr)

; ---- Set<T> ----
declare ptr @Set_new()
declare void @Set_insert(ptr, i64)
declare i1 @Set_contains(ptr, i64)
declare void @Set_remove(ptr, i64)
declare i64 @Set_len(ptr)
declare void @Set_for_each(ptr, ptr)
declare ptr @Set_intersection(ptr, ptr)
declare ptr @Set_union(ptr, ptr)
declare ptr @Set_difference(ptr, ptr)
declare i1 @Set_is_subset(ptr, ptr)
declare i1 @Set_is_superset(ptr, ptr)
declare i1 @Set_is_disjoint(ptr, ptr)

; ---- Deque<T> ----
declare ptr @Deque_new()
declare void @Deque_push_front(ptr, i64)
declare void @Deque_push_back(ptr, i64)
declare i64 @Deque_pop_front(ptr)
declare i64 @Deque_pop_back(ptr)
declare i64 @Deque_front(ptr)
declare i64 @Deque_back(ptr)
declare i64 @Deque_len(ptr)
declare i1 @Deque_is_empty(ptr)
declare i1 @Deque_contains(ptr, i64)

; ---- TernaryTrie ----
declare ptr @TernaryTrie_new()
declare void @TernaryTrie_insert(ptr, ptr, i64)
declare i64 @TernaryTrie_get(ptr, ptr)
declare i1 @TernaryTrie_contains(ptr, ptr)
declare i64 @TernaryTrie_len(ptr)
declare ptr @TernaryTrie_keys_with_prefix(ptr, ptr)

; ---- Result types ----
declare ptr @Ok_new(i64)
declare ptr @Err_new(ptr)
declare ptr @Unknown_new(ptr)
declare i1 @result_is_ok(ptr)
declare i1 @result_is_err(ptr)
declare i1 @result_is_unknown(ptr)
declare i64 @result_unwrap(ptr)

; ---- sync / concurrency ----
declare ptr @Mutex_new(i64)
declare void @Mutex_lock(ptr)
declare void @Mutex_unlock(ptr)
declare i64 @Mutex_get(ptr)
declare void @Mutex_set(ptr, i64)
declare ptr @AtomicTrit_new(i8)
declare i8 @AtomicTrit_get(ptr)
declare void @AtomicTrit_set(ptr, i8)
declare i8 @AtomicTrit_swap(ptr, i8)
declare i1 @AtomicTrit_compare_exchange(ptr, i8, i8)
declare i8 @AtomicTrit_fetch_and(ptr, i8)
declare i8 @AtomicTrit_fetch_or(ptr, i8)
declare i8 @AtomicTrit_fetch_neg(ptr)
declare ptr @AtomicInt_new(i64)
declare i64 @AtomicInt_load(ptr)
declare void @AtomicInt_store(ptr, i64)
declare i64 @AtomicInt_swap(ptr, i64)
declare i64 @AtomicInt_fetch_add(ptr, i64)
declare i64 @AtomicInt_fetch_sub(ptr, i64)
declare i1 @AtomicInt_compare_exchange(ptr, i64, i64)
declare ptr @channel_new()
declare ptr @channel_bounded(i64)
declare void @channel_send(ptr, i64)
declare i64 @channel_recv(ptr)
declare i64 @channel_len(ptr)
declare i1 @channel_is_empty(ptr)
declare void @channel_close(ptr)
declare i1 @channel_is_closed(ptr)
declare void @async_yield_now()
declare ptr @manit_spawn(ptr, i64)
declare i64 @manit_join(ptr)
declare ptr @Semaphore_new(i64)
declare void @Semaphore_acquire(ptr)
declare void @Semaphore_release(ptr)
declare i1 @Semaphore_try_acquire(ptr)
declare i64 @Semaphore_available(ptr)
declare ptr @Barrier_new(i64)
declare i1 @Barrier_wait(ptr)
declare i64 @Barrier_count(ptr)

; ---- ternary utils ----
declare i64 @ternary_trit_to_int(i8)
declare i8 @ternary_int_to_trit(i64)
declare ptr @ternary_t27_to_str(i64)
declare ptr @ternary_to_balanced_ternary(i64)
declare i64 @ternary_from_balanced_ternary(ptr)

; ---- fs (filesystem) ----
declare i1 @fs_exists(ptr)
declare i1 @fs_is_file(ptr)
declare i64 @fs_is_dir(ptr)
declare ptr @fs_read_file(ptr)
declare void @fs_write_file(ptr, ptr)
declare void @fs_append_file(ptr, ptr)
declare void @fs_remove_file(ptr)
declare void @fs_copy_file(ptr, ptr)
declare void @fs_rename(ptr, ptr)
declare void @fs_create_dir(ptr)
declare void @fs_create_dir_all(ptr)
declare void @fs_remove_dir(ptr)
declare i64 @fs_file_size(ptr)
declare i64 @fs_list_dir_open(ptr)
declare ptr @fs_list_dir_entry(i64)
declare i64 @fs_copy(ptr, ptr)
declare i64 @fs_move(ptr, ptr)
declare ptr @fs_open(ptr, ptr)
declare ptr @fs_read(ptr, i64)
declare ptr @fs_read_line(ptr)
declare void @fs_write(ptr, ptr)
declare void @fs_close(ptr)
declare void @fs_flush(ptr)
declare void @fs_seek(ptr, i64)
declare i64 @fs_tell(ptr)

; ---- env (environment) ----
declare ptr @env_get_env(ptr)
declare ptr @env_get_env_or(ptr, ptr)
declare void @env_set_env(ptr, ptr)
declare void @env_unset_env(ptr)
declare i1 @env_has_env(ptr)
declare i64 @env_argc()
declare ptr @env_argv(i64)
declare ptr @env_cwd()
declare void @env_set_cwd(ptr)
declare i64 @env_pid()
declare i64 @env_ppid()
declare ptr @env_os_name()
declare ptr @env_arch()
declare i64 @env_cpu_count()
declare ptr @env_home_dir()
declare ptr @env_temp_dir()
declare void @env_exit(i64)
declare void @env_abort(ptr)
declare ptr @env_timestamp()
declare ptr @env_date()
declare ptr @env_time()
declare i64 @process_spawn(ptr)
declare ptr @shell_exec(ptr)

; ---- time ----
declare i64 @time_now_ms()
declare i64 @time_now_nanos()
declare void @time_sleep_ms(i64)
declare i64 @time_unix_secs()
declare ptr @time_format_iso8601(i64)

; ---- path utilities ----
declare ptr @path_join(ptr, ptr)
declare ptr @path_file_name(ptr)
declare ptr @path_extension(ptr)
declare ptr @path_parent(ptr)

; ---- alternate name aliases (IR lowerer may use module::func naming) ----
declare i64 @math_from_balanced_ternary(ptr)
declare ptr @math_to_balanced_ternary(i64)
declare ptr @math_trits_to_str(i64)
declare ptr @int_to_str(i64)
declare ptr @float_to_str(double)
declare i64 @ternary_t27_shift_left(i64, i64)
declare i64 @ternary_t27_shift_right(i64, i64)
declare i64 @ternary_t27_rotate_left(i64, i64)
declare i64 @ternary_t27_rotate_right(i64, i64)
declare ptr @Ok(i64)
declare ptr @Err(ptr)
declare ptr @Unknown(ptr)
declare ptr @str_slice(ptr, i64, i64)
declare ptr @str_repeat(ptr, i64)
declare i64 @str_index_of(ptr, ptr)
declare ptr @str_chars(ptr)
declare ptr @str_join(ptr, ptr)
declare ptr @str_reverse(ptr)
declare ptr @str_from_float(double)
declare ptr @str_from_bool(i1)
declare ptr @str_from_trit(i8)
declare i8 @ternary_trit_median(i8, i8, i8)
declare ptr @str_to_int(ptr)
declare ptr @str_to_float(ptr)
declare void @Channel_send(ptr, i64)
declare i64 @Channel_recv(ptr)
declare i64 @Channel_len(ptr)
declare i1 @Channel_is_empty(ptr)
declare void @Channel_close(ptr)
declare i1 @Channel_is_closed(ptr)
declare ptr @Channel_new()
declare ptr @Channel_bounded(i64)
declare ptr @channel()
declare i1 @TernaryTrie_contains_key(ptr, ptr)
declare void @TernaryTrie_remove(ptr, ptr)
declare ptr @TernaryTrie_keys(ptr)

; ---- terminal control (TUI) ----
declare i64 @terminal_set_raw()
declare i64 @terminal_set_cooked()
declare i64 @terminal_get_rows()
declare i64 @terminal_get_cols()
declare i64 @io_read_char()
declare i64 @io_read_key()
declare i64 @io_clear_screen()
declare i64 @io_move_cursor(i64, i64)
declare i64 @io_set_reverse()
declare i64 @io_reset_attr()
declare i64 @io_set_bold()

; ---- http (libcurl) ----
declare ptr @net_http_get(ptr)

; ---- SDL2 GUI ----
declare i64 @gui_init(i64, i64, ptr)
declare i64 @gui_quit()
declare i64 @gui_clear()
declare i64 @gui_present()
declare i64 @gui_set_color(i64, i64, i64, i64)
declare i64 @gui_fill_rect(i64, i64, i64, i64)
declare i64 @gui_draw_rect(i64, i64, i64, i64)
declare i64 @gui_draw_line(i64, i64, i64, i64)
declare i64 @gui_draw_text(ptr, i64, i64)
declare i64 @gui_draw_text_lg(ptr, i64, i64)
declare i64 @gui_text_width(ptr)
declare i64 @gui_font_height()
declare i64 @gui_window_width()
declare i64 @gui_window_height()
declare i64 @gui_poll_event()
declare i64 @gui_wait_event(i64)
declare i64 @gui_event_type()
declare i64 @gui_event_key()
declare i64 @gui_mouse_x()
declare i64 @gui_mouse_y()
declare i64 @gui_mouse_btn()
declare i64 @gui_event_text_char()
declare i64 @gui_ticks()
declare i64 @gui_delay(i64)
declare i64 @gui_key_return()
declare i64 @gui_key_escape()
declare i64 @gui_key_backspace()
declare i64 @gui_key_delete()
declare i64 @gui_key_up()
declare i64 @gui_key_down()
declare i64 @gui_key_left()
declare i64 @gui_key_right()
declare i64 @gui_key_home()
declare i64 @gui_key_end()
declare i64 @gui_key_pageup()
declare i64 @gui_key_pagedown()
declare i64 @gui_key_f1()
declare i64 @gui_key_f2()
declare i64 @gui_key_f3()
declare i64 @gui_key_f4()
declare i64 @gui_key_f5()
declare i64 @gui_key_f6()
declare i64 @gui_key_f7()
declare i64 @gui_key_f8()
declare i64 @gui_key_f9()
declare i64 @gui_key_f10()
declare i64 @gui_key_f11()
declare i64 @gui_key_f12()
declare i64 @gui_key_tab()
declare i64 @gui_key_space()
declare i64 @gui_key_a()
declare i64 @gui_key_b()
declare i64 @gui_key_c()
declare i64 @gui_key_d()
declare i64 @gui_key_e()
declare i64 @gui_key_f()
declare i64 @gui_key_g()
declare i64 @gui_key_h()
declare i64 @gui_key_i()
declare i64 @gui_key_j()
declare i64 @gui_key_k()
declare i64 @gui_key_l()
declare i64 @gui_key_m()
declare i64 @gui_key_n()
declare i64 @gui_key_o()
declare i64 @gui_key_p()
declare i64 @gui_key_q()
declare i64 @gui_key_r()
declare i64 @gui_key_s()
declare i64 @gui_key_t()
declare i64 @gui_key_u()
declare i64 @gui_key_v()
declare i64 @gui_key_w()
declare i64 @gui_key_x()
declare i64 @gui_key_y()
declare i64 @gui_key_z()
declare i64 @gui_key_mod_ctrl()
declare i64 @gui_key_mod_shift()
declare i64 @gui_key_mod_alt()
declare ptr @gui_event_text_str()
declare i64 @gui_wheel_dy()
declare i64 @fs_mkdir(ptr)
declare ptr @gui_clipboard_get()
declare void @gui_clipboard_set(ptr)
declare i64 @fs_delete(ptr)

";

// ---------------------------------------------------------------------------
// String escaping for LLVM IR constant strings
// ---------------------------------------------------------------------------

/// Escape a Rust string into the LLVM IR `c"..."` constant syntax.
/// Non-printable / non-ASCII bytes become `\XX` (uppercase hex).
pub(crate) fn llvm_escape_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for byte in s.bytes() {
        match byte {
            // Printable ASCII except '"' (0x22) and '\' (0x5C)
            0x20..=0x21 | 0x23..=0x5B | 0x5D..=0x7E => {
                out.push(byte as char);
            }
            b => {
                out.push('\\');
                out.push(char::from_digit((b >> 4) as u32, 16).unwrap().to_ascii_uppercase());
                out.push(char::from_digit((b & 0xF) as u32, 16).unwrap().to_ascii_uppercase());
            }
        }
    }
    out
}

