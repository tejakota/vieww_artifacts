//! The out-of-process preview's child: loads a compiled screen and draws it
//! for the studio over stdin/stdout. See `viewwstudio::remote`.
//!
//! Not for running by hand — it speaks a binary protocol on its pipes.

fn main() -> std::io::Result<()> {
    viewwstudio::remote::serve_stdio()
}
