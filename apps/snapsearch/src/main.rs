//! Desktop entry point — and, per `vieww_platform_winit::examples::ios`,
//! also the correct entry point for iOS: iOS calls `main` itself
//! (`UIApplicationMain` is started from inside `App::run` by winit), so
//! there is no `run_ios` the way there is a `run_android`.

use vieww_platform_winit::App;

fn main() {
    let (app, slot) = snapsearch::app::configure(App::new());
    let result = app.run(move |driver| snapsearch::app::build_root(driver, &slot));
    match result {
        Ok(report) => println!("{report}"),
        Err(err) => eprintln!("SnapSearch failed to start: {err}"),
    }
}
