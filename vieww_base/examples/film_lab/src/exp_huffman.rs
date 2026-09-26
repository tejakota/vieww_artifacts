//! exp_huffman — *the information axis.* How few bits a sentence needs.
//!
//! In 1951 David Huffman, a graduate student who took the term paper
//! instead of the final exam, proved that the optimal prefix code is built
//! by repeatedly merging the two least likely symbols. This plate builds
//! that tree in front of you, from a fixed passage, one merge per beat, and
//! then audits the result against the two laws that bound it.
//!
//! **Shannon's law.** No prefix code can have a mean length below the
//! entropy H = −Σ p log₂ p, and Huffman's is guaranteed to come within one
//! bit of it: **H ≤ L < H + 1**. The plate measures H from the passage's own
//! letter counts, measures L as Σ pᵢ·lᵢ over the code it actually built, and
//! prints the gap.
//!
//! **Kraft's equality.** For any *complete* prefix code, Σ 2^(−lᵢ) = 1
//! exactly — the code leaves no address space unspent. A Huffman code is
//! always complete, so this is not an approximation to check but an identity
//! to confirm, and the residual printed is a floating-point one.
//!
//! Beside them: the compression actually obtained against a flat 8-bit
//! byte, the longest and shortest codewords with the symbols that own them,
//! and the entropy of the *encoded* bit stream, which must be ≈ 1.0 bits per
//! bit if the coder did its job — a compressed stream should look like
//! noise, and the plate measures how close to noise this one looks.

use vieww_foundation::{BlendMode, Color, Gradient, Offset, Path, Rect, Size, Sketchbook,
    TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith, Text};

use crate::film_lib::{alpha, mix, AMBER, CYAN, CYAN_SOFT, INK, MINT, MUTED, VIOLET};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 12.0;

/// The passage. Fixed in the source, so the counts, the tree, the code and
/// every number below are properties of this file and reproduce exactly.
const TEXT: &str = "the receipts are the spine of this laboratory. \
every number printed on a plate is measured from the arrays that drew the \
frame, and a plate whose receipt disagrees with its own law is a broken \
plate however good it looks. the audit layer measures looks; the receipt \
layer measures truth; the pixel census settles disputes between them.";

// ── The tree ────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct Node {
    weight: usize,
    symbol: Option<char>,
    left: Option<usize>,
    right: Option<usize>,
    /// The merge step this node was created on (leaves: 0).
    born: usize,
}

struct Code {
    nodes: Vec<Node>,
    root: usize,
    /// (symbol, count, code string), in descending frequency.
    table: Vec<(char, usize, String)>,
    merges: usize,
}

fn build(merges_to: usize) -> Code {
    // ── the census ──
    let mut counts: Vec<(char, usize)> = Vec::new();
    for ch in TEXT.chars() {
        match counts.iter_mut().find(|(c, _)| *c == ch) {
            Some((_, n)) => *n += 1,
            None => counts.push((ch, 1)),
        }
    }
    // A deterministic order before any merging: by count, then by codepoint.
    // Ties in a Huffman queue are resolved by *something*, and if that
    // something is hash order the tree is different on every run and the
    // plate is not reproducible. This is the tie-break, stated.
    counts.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));

    let mut nodes: Vec<Node> = counts
        .iter()
        .map(|&(c, w)| Node {
            weight: w,
            symbol: Some(c),
            left: None,
            right: None,
            born: 0,
        })
        .collect();
    let mut live: Vec<usize> = (0..nodes.len()).collect();

    let total_merges = live.len().saturating_sub(1);
    let stop = merges_to.min(total_merges);
    for m in 0..stop {
        // the two lightest, with the same stated tie-break
        live.sort_by(|&a, &b| {
            nodes[a]
                .weight
                .cmp(&nodes[b].weight)
                .then(nodes[a].born.cmp(&nodes[b].born))
                .then(a.cmp(&b))
        });
        let a = live.remove(0);
        let b = live.remove(0);
        let n = Node {
            weight: nodes[a].weight + nodes[b].weight,
            symbol: None,
            left: Some(a),
            right: Some(b),
            born: m + 1,
        };
        nodes.push(n);
        live.push(nodes.len() - 1);
    }
    let root = *live
        .iter()
        .max_by_key(|&&i| nodes[i].weight)
        .unwrap_or(&0);

    // ── the codes, walked out of the finished tree ──
    let mut table: Vec<(char, usize, String)> = Vec::new();
    if stop == total_merges {
        fn walk(nodes: &[Node], i: usize, prefix: String, out: &mut Vec<(char, usize, String)>) {
            if let Some(c) = nodes[i].symbol {
                out.push((c, nodes[i].weight, if prefix.is_empty() { "0".into() } else { prefix }));
                return;
            }
            if let Some(l) = nodes[i].left {
                walk(nodes, l, format!("{prefix}0"), out);
            }
            if let Some(r) = nodes[i].right {
                walk(nodes, r, format!("{prefix}1"), out);
            }
        }
        walk(&nodes, root, String::new(), &mut table);
        table.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    }

    Code {
        nodes,
        root,
        table,
        merges: stop,
    }
}

/// Lay the finished tree out for drawing: x by in-order position, y by
/// depth. Returns (x, y, depth) per node index, and the max depth.
fn layout(code: &Code) -> (Vec<(f32, f32)>, usize) {
    let n = code.nodes.len();
    let mut xy = vec![(0.0_f32, 0.0_f32); n];
    let mut next_x = 0.0_f32;
    let mut max_d = 0usize;
    fn walk(
        nodes: &[Node],
        i: usize,
        d: usize,
        next_x: &mut f32,
        xy: &mut [(f32, f32)],
        max_d: &mut usize,
    ) {
        *max_d = (*max_d).max(d);
        match (nodes[i].left, nodes[i].right) {
            (Some(l), Some(r)) => {
                walk(nodes, l, d + 1, next_x, xy, max_d);
                let lx = xy[l].0;
                walk(nodes, r, d + 1, next_x, xy, max_d);
                let rx = xy[r].0;
                xy[i] = ((lx + rx) * 0.5, d as f32);
            }
            _ => {
                xy[i] = (*next_x, d as f32);
                *next_x += 1.0;
            }
        }
    }
    walk(&code.nodes, code.root, 0, &mut next_x, &mut xy, &mut max_d);
    (xy, max_d)
}

// ── The frame ───────────────────────────────────────────────────────────────

pub fn frame(t: f32) -> WidgetNode {
    // The film builds the tree, then holds the finished code.
    let full = build(usize::MAX);
    let total_merges = full.merges;
    let shown = ((t as f64 * (total_merges as f64 * 1.28)) as usize).min(total_merges);
    let partial = build(shown);
    let finished = shown == total_merges;

    let n_symbols = full.table.len();
    let n_chars = TEXT.chars().count();

    // ── Shannon's H, from the passage's own counts ──
    let h: f64 = full
        .table
        .iter()
        .map(|&(_, c, _)| {
            let p = c as f64 / n_chars as f64;
            -p * p.log2()
        })
        .sum();
    // ── L, from the code actually built ──
    let l: f64 = full
        .table
        .iter()
        .map(|&(_, c, ref code)| c as f64 / n_chars as f64 * code.len() as f64)
        .sum();
    // ── Kraft ──
    let kraft: f64 = full
        .table
        .iter()
        .map(|&(_, _, ref code)| 2.0_f64.powi(-(code.len() as i32)))
        .sum();
    let total_bits: usize = full
        .table
        .iter()
        .map(|&(_, c, ref code)| c * code.len())
        .sum();
    let flat_bits = n_chars * 8;

    // ── the encoded stream, and how close to noise it looks ──
    let mut stream = String::with_capacity(total_bits);
    for ch in TEXT.chars() {
        if let Some((_, _, code)) = full.table.iter().find(|(c, _, _)| *c == ch) {
            stream.push_str(code);
        }
    }
    let ones = stream.chars().filter(|&c| c == '1').count();
    let p1 = ones as f64 / stream.len().max(1) as f64;
    let bit_entropy = if p1 > 0.0 && p1 < 1.0 {
        -(p1 * p1.log2() + (1.0 - p1) * (1.0 - p1).log2())
    } else {
        0.0
    };

    let longest = full.table.iter().map(|(_, _, c)| c.len()).max().unwrap_or(0);
    let shortest = full.table.iter().map(|(_, _, c)| c.len()).min().unwrap_or(0);
    let long_sym = full
        .table
        .iter()
        .filter(|(_, _, c)| c.len() == longest)
        .map(|(c, _, _)| show(*c))
        .collect::<Vec<_>>()
        .join(" ");
    let short_sym = full
        .table
        .iter()
        .filter(|(_, _, c)| c.len() == shortest)
        .map(|(c, _, _)| show(*c))
        .collect::<Vec<_>>()
        .join(" ");

    let (xy, max_d) = layout(&partial);
    let nodes = partial.nodes.clone();
    let root = partial.root;
    let table = full.table.clone();
    let stream_head: String = stream.chars().take(1400).collect();

    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            book.rect(
                Rect::new(0.0, 0.0, size.width, size.height),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(6, 6, 10)),
                    (1.0, Color::rgb(12, 11, 16)),
                ]),
            );

            // ── the tree ──
            let tx = 46.0_f32;
            let ty = 178.0_f32;
            let tw = 700.0_f32;
            let th = 330.0_f32;
            book.rrect(
                Rect::new(tx - 14.0, ty - 28.0, tx + tw + 14.0, ty + th + 18.0),
                10.0,
                alpha(Color::rgb(13, 13, 19), 0.95),
            );
            let leaves = xy
                .iter()
                .enumerate()
                .filter(|(i, _)| nodes[*i].left.is_none())
                .count()
                .max(2);
            let span = xy.iter().map(|p| p.0).fold(1.0_f32, f32::max).max(1.0);
            let px = |i: usize| tx + xy[i].0 / span * tw;
            let py = |i: usize| ty + xy[i].1 / (max_d.max(1)) as f32 * th;
            let _ = leaves;

            let wmax = nodes[root].weight as f32;
            book.blended_layer(1.0, 0.0, BlendMode::Plus, None, |g| {
                for (i, nd) in nodes.iter().enumerate() {
                    if xy[i] == (0.0, 0.0) && i != root {
                        continue;
                    }
                    for (child, bit) in [(nd.left, 0u8), (nd.right, 1u8)] {
                        if let Some(c) = child {
                            let k = (nodes[c].weight as f32 / wmax).powf(0.4);
                            g.line(
                                Offset::new(px(i), py(i)),
                                Offset::new(px(c), py(c)),
                                alpha(if bit == 0 { CYAN } else { AMBER }, 0.18 + 0.55 * k),
                                0.8 + 2.6 * k,
                            );
                        }
                    }
                }
            });
            for (i, nd) in nodes.iter().enumerate() {
                if xy[i] == (0.0, 0.0) && i != root {
                    continue;
                }
                let k = (nd.weight as f32 / wmax).powf(0.4);
                let c = if nd.symbol.is_some() { VIOLET } else { MINT };
                book.circle(
                    Offset::new(px(i), py(i)),
                    1.6 + 3.4 * k,
                    alpha(c, 0.55 + 0.4 * k),
                );
            }

            // ── the code table ──
            let cx = 786.0_f32;
            let cy = 178.0_f32;
            let cw = 448.0_f32;
            let chh = 330.0_f32;
            book.rrect(
                Rect::new(cx - 16.0, cy - 28.0, cx + cw + 16.0, cy + chh + 18.0),
                10.0,
                alpha(Color::rgb(13, 13, 19), 0.95),
            );
            let n_show = table.len().min(26);
            let fmax = table.first().map(|t| t.1).unwrap_or(1) as f32;
            for (r, (_, count, code)) in table.iter().take(n_show).enumerate() {
                let y = cy + r as f32 * (chh / n_show as f32);
                let rowh = chh / n_show as f32 - 1.5;
                // the frequency bar
                book.rect(
                    Rect::new(
                        cx + 120.0,
                        y,
                        cx + 120.0 + *count as f32 / fmax * 150.0,
                        y + rowh,
                    ),
                    alpha(VIOLET, 0.5),
                );
                // the codeword, as its own bits
                for (b, ch) in code.chars().enumerate() {
                    book.rect(
                        Rect::new(
                            cx + 286.0 + b as f32 * 9.0,
                            y + 1.0,
                            cx + 286.0 + b as f32 * 9.0 + 7.0,
                            y + rowh - 1.0,
                        ),
                        alpha(if ch == '1' { AMBER } else { CYAN_SOFT }, 0.85),
                    );
                }
            }

            // ── the encoded stream ──
            let sx = 46.0_f32;
            let sy = 566.0_f32;
            let sw = 1188.0_f32;
            let sh = 96.0_f32;
            book.rrect(
                Rect::new(sx - 14.0, sy - 28.0, sx + sw + 14.0, sy + sh + 16.0),
                10.0,
                alpha(Color::rgb(13, 13, 19), 0.95),
            );
            let per_row = 340usize;
            let bw = sw / per_row as f32;
            let rh = sh / 4.0;
            for (i, ch) in stream_head.chars().enumerate() {
                let r = i / per_row;
                if r >= 4 {
                    break;
                }
                let c = i % per_row;
                book.rect(
                    Rect::new(
                        sx + c as f32 * bw,
                        sy + r as f32 * rh,
                        sx + c as f32 * bw + bw * 0.78,
                        sy + r as f32 * rh + rh * 0.72,
                    ),
                    alpha(if ch == '1' { AMBER } else { Color::rgb(40, 52, 72) }, 0.9),
                );
            }
        }),
    );

    let lines = vec![
        "HUFFMAN · THE INFORMATION AXIS · THE FEWEST BITS A SENTENCE CAN HAVE".to_string(),
        format!(
            "a fixed {n_chars}-character passage, {n_symbols} distinct symbols · the queue's tie-break is stated in the source (weight, then birth, then index) so the tree is reproducible"
        ),
        format!(
            "building: merge {shown}/{total_merges}{} · every merge takes the two lightest live nodes and makes them siblings — that rule alone is the optimality proof",
            if finished { " — code complete" } else { "" }
        ),
        format!(
            "SHANNON: H = −Σ p·log₂p = {h:.4} bits/symbol · HUFFMAN: L = Σ p·lᵢ = {l:.4} bits/symbol · the gap L − H = {:.4} bits, inside the guaranteed H ≤ L < H+1",
            l - h
        ),
        format!(
            "KRAFT: Σ 2^(−lᵢ) = {kraft:.12} (residual {:.2e}) — the code is complete: it spends its address space exactly, with nothing left over",
            (kraft - 1.0).abs()
        ),
        format!(
            "THE BILL: {total_bits} bits vs {flat_bits} flat 8-bit bytes = {:.1}% of the size ({:.2}× compression) · codewords {shortest}–{longest} bits (shortest: {short_sym} · longest: {long_sym})",
            total_bits as f32 / flat_bits as f32 * 100.0,
            flat_bits as f32 / total_bits as f32
        ),
        format!(
            "THE STREAM ITSELF: {:.4} of its bits are ones → per-bit entropy {bit_entropy:.6}, i.e. {:.4} bits short of pure noise, which is what a well-coded stream should look like",
            p1,
            1.0 - bit_entropy
        ),
    ];

    let mut stack = Stack::new().push(Positioned::fill().child(board));
    for (i, line) in lines.iter().enumerate() {
        stack = stack.push(
            Positioned::new()
                .left(42.0)
                .top(26.0 + i as f32 * 16.0)
                .width(1200.0)
                .height(15.0)
                .child(
                    Text::new(line.clone()).style(
                        TextStyle::new(if i == 0 { 12.0 } else { 11.0 })
                            .monospace()
                            .letter_spacing(if i == 0 { 1.8 } else { 0.0 })
                            .color(alpha(if i == 0 { MUTED } else { mix(MUTED, INK, 0.45) }, 0.95)),
                    ),
                ),
        );
    }
    // the table's own labels — symbol glyphs drawn as text beside the bars
    let table_rows = full.table.len().min(26);
    for (r, (ch, count, code)) in full.table.iter().take(table_rows).enumerate() {
        let y = 178.0 + r as f32 * (330.0 / table_rows as f32) - 2.0;
        stack = stack.push(
            Positioned::new().left(790.0).top(y).width(130.0).height(13.0).child(
                Text::new(format!("{:>4}  {:>4}  {:>2}b", show(*ch), count, code.len())).style(
                    TextStyle::new(9.5)
                        .monospace()
                        .color(alpha(mix(MUTED, INK, 0.5), 0.95)),
                ),
            ),
        );
    }
    for (x, y, s) in [
        (46.0_f32, 152.0_f32, "THE TREE — merge by merge · cyan edges are 0, amber edges are 1 · node size is weight".to_string()),
        (786.0, 152.0, "THE CODE — the 26 most frequent of the 27 symbols · count, length, frequency, codeword".to_string()),
        (46.0, 540.0, "THE ENCODED STREAM — the passage itself, as bits".to_string()),
    ] {
        stack = stack.push(
            Positioned::new().left(x).top(y).width(700.0).height(14.0).child(
                Text::new(s).style(
                    TextStyle::new(9.5)
                        .monospace()
                        .letter_spacing(0.9)
                        .color(alpha(MUTED, 0.85)),
                ),
            ),
        );
    }
    stack.into()
}

fn show(c: char) -> String {
    match c {
        ' ' => "␣".to_string(),
        '\n' => "⏎".to_string(),
        other => other.to_string(),
    }
}
