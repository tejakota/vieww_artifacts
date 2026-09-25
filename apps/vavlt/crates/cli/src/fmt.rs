//! Output formatting. Kept dull on purpose — these numbers get pasted into
//! the spec, so they must be easy to read and hard to misread.

pub fn bytes(n: u64) -> String {
    const U: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{n} B")
    } else {
        format!("{v:.1} {}", U[i])
    }
}

pub fn rule(width: usize) -> String {
    "-".repeat(width)
}

pub fn header(title: &str) {
    println!("\n{title}");
    println!("{}", rule(title.len()));
}
