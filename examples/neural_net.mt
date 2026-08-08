// examples/neural_net.mt
// Ternary Neural Network Inference (Thatte10 NPU concept)
//
// In a ternary neural network, weights are trits {-1, 0, +1}.
// This eliminates multiplication entirely:
//   weight +1: pass the input unchanged        (just wire it through)
//   weight  0: ignore the input                (disconnect — zero energy)
//   weight -1: negate the input                (trit-flip — tnot, one gate)
//
// No multiplier needed.  No floating-point unit.  No MAC energy budget.
// A "multiply-accumulate" becomes "route-and-add" — the fundamental
// operation of the Thatte10 NPU.
//
// This is not a compromise: ternary-weight neural networks achieve
// near-full-precision accuracy on many tasks, because {-1, 0, +1}
// captures the essential structure of learned weight matrices.
//
// Demonstrates:
//   - Ternary MAC (multiply-accumulate) cell
//   - 3-neuron, 2-layer perceptron with ternary weights
//   - Inference on a simple input pattern
//   - Why ternary multiplication is free
//   - Activation function (ternary step / sign)
//
// Authored by: Manish Jagdish Thatte

use std::io;
use std::fmt;
use std::math;
use std::ternary;
use std::collections;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------
fn heading(title: str) {
    io::println("");
    io::println("================================================");
    io::print("  ");
    io::println(title);
    io::println("================================================");
}

fn print_vec(label: str, v: Vec<int>) {
    io::print(label);
    io::print("[");
    let mut i = 0;
    let n = v.len();
    while i < n {
        io::print_int(v.get(i));
        if i < n - 1 { io::print(", "); }
        i = i + 1;
    }
    io::println("]");
}

fn print_weights(label: str, w: Vec<trit>) {
    io::print(label);
    io::print("[");
    let mut i = 0;
    let n = w.len();
    while i < n {
        io::print_trit(w.get(i));
        if i < n - 1 { io::print(", "); }
        i = i + 1;
    }
    io::println("]");
}

// ---------------------------------------------------------------------------
// 1. Ternary Multiply-Accumulate (MAC) Cell
// ---------------------------------------------------------------------------
// The MAC cell is the core of any neural network hardware.
// In binary: MAC = a * w + acc  (requires a hardware multiplier)
// In ternary: MAC = trit_select(w, a) + acc  (no multiplier!)
//
// trit_select(w, a):
//   w = +  =>  +a    (pass through)
//   w = 0  =>   0    (ignore — saves energy)
//   w = -  =>  -a    (negate — one tnot gate)

fn trit_mac(weight: trit, input: int) -> int {
    // "Multiplication" by a trit — zero-cost in hardware.
    tif weight {
        + => return input,      // wire through
        0 => return 0,          // disconnect
        - => return 0 - input,  // negate (one gate)
    }
}

fn demo_mac_cell() {
    heading("1. Ternary MAC Cell — Multiplication is Free");

    io::println("  Ternary weight * input (no multiplier needed):");
    io::println("");
    io::println("    weight  |  input  |  output  |  operation");
    io::println("    --------|---------|----------|---------------------");

    let weights: [trit] = [+, 0, -];
    let weight_ops: [str] = ["pass (wire)", "ignore (zero)", "negate (tnot)"];
    let input = 42;

    let mut i = 0;
    for w in weights {
        let result = trit_mac(w, input);
        io::print("      ");
        io::print_trit(w);
        io::print("     |   ");
        io::print(fmt::align_right(fmt::show_int(input), 4, ' '));
        io::print("  |   ");
        io::print(fmt::align_right(fmt::show_int(result), 4, ' '));
        io::print("   |  ");
        io::println(weight_ops[i]);
        i = i + 1;
    }

    io::println("");
    io::println("  Binary MAC: requires a multiplier circuit (hundreds of gates).");
    io::println("  Ternary MAC: a wire, a zero, or a tnot gate. That's it.");
    io::println("  Energy savings: orders of magnitude per inference.");
}

// ---------------------------------------------------------------------------
// 2. Neuron — Dot Product + Activation
// ---------------------------------------------------------------------------
// A neuron computes: output = activate(sum of weight_i * input_i + bias)
// With ternary weights, the "sum of weight_i * input_i" becomes
// "sum of trit_select(weight_i, input_i)" — all routing, no multiplication.

// Ternary step activation: maps accumulated value to {-1, 0, +1}.
// This is the ternary analogue of the sign function.
// Threshold region around zero maps to 0 (the Unknown / undecided state).
fn ternary_activate(x: int, threshold: int) -> int {
    if x > threshold  { return 1; }
    if x < 0 - threshold { return -1; }
    return 0;
}

// Compute a single neuron's output.
fn neuron(weights: Vec<trit>, inputs: Vec<int>, bias: int, threshold: int) -> int {
    let n = weights.len();
    let mut acc = bias;
    let mut i = 0;
    while i < n {
        acc = acc + trit_mac(weights.get(i), inputs.get(i));
        i = i + 1;
    }
    return ternary_activate(acc, threshold);
}

fn demo_neuron() {
    heading("2. Single Neuron — Ternary Dot Product + Activation");

    // 4 inputs, ternary weights.
    let weights: Vec<trit> = Vec::new();
    weights.push(+);   // pass input 0
    weights.push(-);   // negate input 1
    weights.push(0);   // ignore input 2
    weights.push(+);   // pass input 3

    let inputs: Vec<int> = Vec::new();
    inputs.push(3);
    inputs.push(5);
    inputs.push(7);
    inputs.push(2);

    let bias = 0;
    let threshold = 2;

    io::println("  Neuron with 4 inputs:");
    print_weights("    Weights: ", weights);
    print_vec("    Inputs:  ", inputs);
    io::print("    Bias:    ");
    io::println_int(bias);
    io::println("");

    // Show the computation step by step.
    io::println("  Computation (no multiplication!):");
    let mut total = bias;
    let mut i = 0;
    while i < 4 {
        let w = weights.get(i);
        let inp = inputs.get(i);
        let contribution = trit_mac(w, inp);
        io::print("    w=");
        io::print_trit(w);
        io::print(" * input=");
        io::print_int(inp);
        io::print("  =>  ");
        io::print_int(contribution);
        io::print("  (");
        tif w {
            + => io::print("pass"),
            0 => io::print("ignore"),
            - => io::print("negate"),
        }
        io::println(")");
        total = total + contribution;
        i = i + 1;
    }

    io::print("    Accumulator: ");
    io::println_int(total);
    let output = ternary_activate(total, threshold);
    io::print("    After activation (threshold=");
    io::print_int(threshold);
    io::print("): ");
    io::println_int(output);
}

// ---------------------------------------------------------------------------
// 3. Two-Layer Perceptron
// ---------------------------------------------------------------------------
// Architecture:
//   Input layer:  4 values
//   Hidden layer: 3 neurons (each with 4 ternary weights)
//   Output layer: 1 neuron  (with 3 ternary weights)
//
// All weights are trits.  The entire network uses zero multiplications.

struct TernaryLayer {
    pub weights: Vec<Vec<trit>>,   // one weight vector per neuron
    pub biases: Vec<int>,
    pub threshold: int,
}

impl TernaryLayer {
    fn new(threshold: int) -> TernaryLayer {
        return TernaryLayer {
            weights: Vec::new(),
            biases: Vec::new(),
            threshold: threshold,
        };
    }

    fn add_neuron(self, w: Vec<trit>, bias: int) {
        self.weights.push(w);
        self.biases.push(bias);
    }

    fn forward(self, inputs: Vec<int>) -> Vec<int> {
        let n_neurons = self.weights.len();
        let outputs: Vec<int> = Vec::new();
        let mut n = 0;
        while n < n_neurons {
            let w = self.weights.get(n);
            let b = self.biases.get(n);
            let out = neuron(w, inputs, b, self.threshold);
            outputs.push(out);
            n = n + 1;
        }
        return outputs;
    }
}

fn make_weight_vec(trits: [trit]) -> Vec<trit> {
    let v: Vec<trit> = Vec::new();
    for t in trits { v.push(t); }
    return v;
}

fn demo_perceptron() {
    heading("3. Two-Layer Ternary Perceptron");

    // Hidden layer: 3 neurons, each with 4 ternary weights.
    let hidden = TernaryLayer::new(1);

    // Neuron H0: detects pattern [+, +, -, -]
    hidden.add_neuron(make_weight_vec([+, +, -, -]), 0);
    // Neuron H1: detects pattern [-, -, +, +]
    hidden.add_neuron(make_weight_vec([-, -, +, +]), 0);
    // Neuron H2: sums all inputs equally
    hidden.add_neuron(make_weight_vec([+, +, +, +]), -1);

    // Output layer: 1 neuron with 3 ternary weights.
    let output_layer = TernaryLayer::new(1);
    // Combines hidden outputs: H0 - H1 + H2
    output_layer.add_neuron(make_weight_vec([+, -, +]), 0);

    io::println("  Architecture: 4 -> 3 -> 1  (all ternary weights)");
    io::println("");
    io::println("  Hidden layer weights:");
    io::print("    H0: "); print_weights("", hidden.weights.get(0));
    io::print("    H1: "); print_weights("", hidden.weights.get(1));
    io::print("    H2: "); print_weights("", hidden.weights.get(2));
    io::println("  Output layer weights:");
    io::print("    O0: "); print_weights("", output_layer.weights.get(0));
    io::println("");

    // Run inference on several input patterns.
    io::println("  Inference on test patterns:");
    io::println("  --------------------------------------------------");

    let patterns: [[int]] = [
        [3, 2, 1, 1],    // positive-heavy, should match H0
        [1, 1, 3, 2],    // negative-heavy pattern for H0, positive for H1
        [2, 2, 2, 2],    // uniform — H2 dominates
        [0, 0, 0, 0],    // all zero — should produce 0
        [5, 4, -3, -2],  // strong asymmetric pattern
    ];

    for pattern in patterns {
        let inp: Vec<int> = Vec::new();
        for v in pattern { inp.push(v); }

        // Forward through hidden layer.
        let h_out = hidden.forward(inp);

        // Forward through output layer.
        let final_out = output_layer.forward(h_out);

        io::print("    Input: ");
        print_vec("", inp);
        io::print("    Hidden: ");
        print_vec("", h_out);
        io::print("    Output: ");
        io::println_int(final_out.get(0));
        io::println("");
    }

    io::println("  Zero multiplications were performed in this entire inference.");
    io::println("  Every 'multiply' was a trit-select: wire, zero, or tnot.");
}

// ---------------------------------------------------------------------------
// 4. Weight Encoding and Storage
// ---------------------------------------------------------------------------
// Ternary weights pack naturally: 5 weights per tryte (3^5 = 243 < 256).
// A t27 stores 27 weights.  A binary system needs 2 bits per ternary
// weight (wasting 25% of storage).

fn demo_weight_storage() {
    heading("4. Weight Encoding — Trits are the Natural Unit");

    io::println("  In binary neural networks:");
    io::println("    Weight precision:  8-bit (int8), 4-bit (int4), or 2-bit");
    io::println("    Storage for {-1,0,+1}: 2 bits (wastes 25%, since 2^2=4 > 3)");
    io::println("    Multiplier:  still needed for multi-bit weights");
    io::println("");
    io::println("  In ternary neural networks (Thatte10 NPU):");
    io::println("    Weight precision:  1 trit (EXACT for {-1, 0, +1})");
    io::println("    Storage: 1 trit = log2(3) = 1.585 bits (zero waste)");
    io::println("    Multiplier:  NOT NEEDED (trit-select replaces it)");
    io::println("");

    // Show packing efficiency.
    io::println("  Weight packing comparison:");
    io::println("    Weights | Binary (2-bit) | Ternary (1-trit) | Savings");
    io::println("    --------|----------------|------------------|--------");

    let counts: [int] = [27, 81, 243, 729];
    for n in counts {
        let binary_bits = n * 2;
        let ternary_trits = n;
        io::print("    ");
        io::print(fmt::align_right(fmt::show_int(n), 7, ' '));
        io::print(" | ");
        io::print(fmt::align_right(fmt::show_int(binary_bits), 10, ' '));
        io::print(" bits | ");
        io::print(fmt::align_right(fmt::show_int(ternary_trits), 12, ' '));
        io::print(" trits | ");
        // Trits are 1.585x denser than bits, but we need n trits vs 2n bits.
        let pct = 100 - (ternary_trits * 100 / binary_bits);
        io::print_int(pct);
        io::println("%");
    }

    io::println("");
    io::println("  A t27 register holds 27 ternary weights — one register, 27 neurons.");
    io::println("  A 32-bit register holds only 16 ternary weights (2 bits each, 25% waste).");
}

// ---------------------------------------------------------------------------
// 5. The Sparsity Advantage
// ---------------------------------------------------------------------------
// Weight 0 means "ignore this input" — the neuron is disconnected from it.
// In binary networks, a zero weight still costs a multiply-by-zero.
// In ternary, weight 0 is literal silence: no signal, no energy.

fn demo_sparsity() {
    heading("5. Sparsity — Weight 0 Costs Nothing");

    io::println("  Ternary weight distribution in a trained network:");
    io::println("  (typical, based on binarization/ternarization literature)");
    io::println("");

    // Simulate a weight distribution.
    let n_weights = 81;
    let weights: Vec<trit> = Vec::new();
    // Typical distribution: ~33% zero, ~33% positive, ~33% negative.
    let pattern: [trit] = [
        +, 0, -, +, -, 0, 0, +, -,
        0, +, +, -, 0, -, +, 0, 0,
        -, 0, +, 0, -, +, +, -, 0,
        +, -, 0, 0, +, 0, -, +, -,
        0, -, +, +, 0, -, 0, +, 0,
        -, +, 0, -, 0, +, -, 0, +,
        0, +, -, 0, +, 0, -, +, -,
        +, 0, -, +, 0, -, 0, +, -,
        0, -, +, 0, -, +, +, 0, -,
    ];

    let mut n_pos = 0;
    let mut n_zer = 0;
    let mut n_neg = 0;
    for w in pattern {
        weights.push(w);
        tif w {
            + => { n_pos = n_pos + 1; },
            0 => { n_zer = n_zer + 1; },
            - => { n_neg = n_neg + 1; },
        }
    }

    io::print("  Total weights:  ");
    io::println_int(n_weights);
    io::print("  Positive (+1):  ");
    io::print_int(n_pos);
    io::print("  (");
    io::print_int(n_pos * 100 / n_weights);
    io::println("%)");
    io::print("  Zero (0):       ");
    io::print_int(n_zer);
    io::print("  (");
    io::print_int(n_zer * 100 / n_weights);
    io::println("%)");
    io::print("  Negative (-1):  ");
    io::print_int(n_neg);
    io::print("  (");
    io::print_int(n_neg * 100 / n_weights);
    io::println("%)");
    io::println("");

    let active_ops = n_pos + n_neg;
    io::print("  Active operations: ");
    io::print_int(active_ops);
    io::print(" (");
    io::print_int(active_ops * 100 / n_weights);
    io::println("%)");
    io::print("  Zero-cost skips:   ");
    io::print_int(n_zer);
    io::print(" (");
    io::print_int(n_zer * 100 / n_weights);
    io::println("%)");
    io::println("");
    io::println("  In binary: even zero weights cost a multiply-by-zero cycle.");
    io::println("  In ternary: weight 0 = disconnected wire. Zero energy. Zero latency.");
    io::println("  The sparsity is FREE, not an optimization — it's the natural state.");
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------
fn main() {
    io::println("maniT Ternary Neural Network Inference");
    io::println("=======================================");
    io::println("Authored by: Manish Jagdish Thatte");

    demo_mac_cell();
    demo_neuron();
    demo_perceptron();
    demo_weight_storage();
    demo_sparsity();

    io::println("");
    io::println("Demo complete.");
}
