//! `3` — reference social application shell built with Vieww.
//!
//! This is deliberately backed by the same `ThreeApp<SocialBackend>` product
//! state that a networked client uses. The bundled backend is offline-first,
//! so the app is runnable without credentials or a server.

use std::{cell::RefCell, fmt, rc::Rc};
use three_app::{HomeFeed, Route, Tab, ThreeApp};
use three_runtime::demo_capture;
use three_social::{demo_profile, MemorySocialBackend, Post, PostId};
use vieww::prelude::*;
use vieww::widget::ElementState;
use vieww_platform_winit::App;
use vieww_integration::ViewerScreen;

fn seeded_app() -> ThreeApp<MemorySocialBackend> {
    let mut backend = MemorySocialBackend::new(demo_profile("teja", "teja", "Teja"));
    backend.add_profile(demo_profile("maya", "maya3d", "Maya"));
    backend.add_profile(demo_profile("noah", "noah.space", "Noah"));
    let mut app = ThreeApp::new(backend);
    app.begin_create();
    app.composer.caption = "A living 3D moment — drag it, rotate it, make it yours.".into();
    app.attach_capture(&demo_capture()).expect("demo capture encodes");
    app.publish_composer().expect("seed post publishes");
    app
}

struct SocialState { app: ThreeApp<MemorySocialBackend>, pending: bool }
impl fmt::Debug for SocialState { fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result{f.debug_struct("SocialState").field("route",&self.app.route).field("feed_items",&self.app.feed.posts.len()).finish()} }
impl SocialState {
    fn new()->Self{Self{app:seeded_app(),pending:false}}
    fn tab(&mut self,tab:Tab){self.app.select_tab(tab);self.pending=true;}
    fn toggle_feed(&mut self){let next=if self.app.home_feed==HomeFeed::ForYou{HomeFeed::Following}else{HomeFeed::ForYou};let _=self.app.set_home_feed(next);self.pending=true;}
    fn like(&mut self,id:&PostId){let _=self.app.like(id);self.pending=true;}
}
impl ElementState for SocialState {
    fn as_any(&self)->&dyn std::any::Any{self}
    fn as_any_mut(&mut self)->&mut dyn std::any::Any{self}
    fn take_pending(&mut self)->bool{std::mem::take(&mut self.pending)}
}
#[derive(Clone)] struct Handle(Rc<RefCell<dyn ElementState>>);
impl Handle { fn write(&self,f:impl FnOnce(&mut SocialState)){let mut s=self.0.borrow_mut();if let Some(s)=s.as_any_mut().downcast_mut::<SocialState>(){f(s)}} }
impl fmt::Debug for Handle { fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result{f.write_str("Handle(<SocialState>)")} }

#[derive(Clone)] struct Snapshot { route:Route, feed:HomeFeed, posts:Vec<Post> }
impl Snapshot { fn read(s:&SocialState)->Self{Self{route:s.app.route.clone(),feed:s.app.home_feed,posts:s.app.feed.posts.clone()}} fn initial()->Self{let app=seeded_app();Self{route:app.route.clone(),feed:app.home_feed,posts:app.feed.posts}} }

#[derive(Debug)] struct SocialScreen;
#[widget]
impl SocialScreen {
    fn create_state(&self)->Option<Box<dyn ElementState>>{Some(Box::new(SocialState::new()))}
    fn build(&self,ctx:&BuildContext)->impl Into<WidgetNode>{
        let theme=ThemeData::of(ctx);
        let snap=ctx.state::<SocialState,Snapshot>(Snapshot::read).unwrap_or_else(Snapshot::initial);
        let handle=ctx.state_handle().map(Handle);
        let body=match &snap.route {
            Route::Root(Tab::Home)=>home_screen(&theme,&snap,handle.clone()),
            Route::Root(Tab::Explore)=>simple_screen(&theme,"Explore","Discover people and spatial moments. Search hooks into the social backend."),
            Route::Root(Tab::Create)|Route::Composer=>simple_screen(&theme,"Create","Capture → reconstruct → preview → caption → privacy → publish .3"),
            Route::Root(Tab::Activity)=>simple_screen(&theme,"Activity","Likes, comments, follows and remixes appear here."),
            Route::Root(Tab::Profile)=>simple_screen(&theme,"@teja","Your .3 posts, followers, following and profile live here."),
            _=>simple_screen(&theme,"3","Detail route"),
        };
        Container::new().color(theme.colors.surface).child(
            Flex::column().cross_axis_alignment(CrossAxisAlignment::Stretch)
                .push(Flexible::expanded(1).child(body))
                .push(navbar(&theme,&snap,handle))
        )
    }
}

fn home_screen(theme:&ThemeData,snap:&Snapshot,handle:Option<Handle>)->WidgetNode{
    let toggle=handle.clone();
    let label=if snap.feed==HomeFeed::ForYou{"For You"}else{"Following"};
    let header=Flex::row().main_axis_alignment(MainAxisAlignment::SpaceBetween)
        .push(Text::new("3").style(theme.text.title).size(28.0).bold())
        .push(Button::new(label).on_pressed(move||{if let Some(h)=&toggle{h.write(|s|s.toggle_feed())}}));

    // The moment card: author, caption, the capture itself and the action
    // row travel together on one raised surface. A feed post is *one thing*;
    // before this it was four siblings floating on the page background, and
    // the viewer's dark chamber bled straight into the surrounding surface
    // with nothing marking whose content it was. Same layout, one surface:
    // radius, fill, and the cast shadow that separates it from the page.
    let card=if let Some(post)=snap.posts.first(){
        let like=handle.clone(); let id=post.id.clone();
        Flex::column().cross_axis_alignment(CrossAxisAlignment::Stretch).spacing(10.0)
            .push(Text::new(format!("@{}",post.author.0)).style(theme.text.label).color(theme.colors.primary).bold())
            .push(Text::new(post.caption.clone()).style(theme.text.body))
            .push(Flexible::expanded(1).child(ViewerScreen{capture:Rc::new(demo_capture())}))
            .push(Flex::row().spacing(8.0)
                .push(Button::new(format!("♥ {}",post.likes)).on_pressed(move||{if let Some(h)=&like{h.write(|s|s.like(&id))}}))
                .push(Button::new(format!("Comment {}",post.comments)))
                .push(Button::new("Remix"))
                .push(Button::new("Share")))
    }else{
        Flex::column().cross_axis_alignment(CrossAxisAlignment::Stretch)
    };
    let card=Container::new()
        .color(theme.colors.surface_variant)
        .radius(18.0)
        .shadow(Shadow::new(Color::rgba(0,0,0,110),Offset::new(0.0,12.0),32.0))
        .padding(EdgeInsets::all(12.0))
        .child(card);

    Flex::column().cross_axis_alignment(CrossAxisAlignment::Stretch).spacing(12.0)
        .push(header)
        .push(Flexible::expanded(1).child(card))
        .into()
}

fn simple_screen(theme:&ThemeData,title:&str,subtitle:&str)->WidgetNode{
    // A hairline separates the title from the body — the one mark a text
    // page needs to read as *designed* rather than typed, and the accent
    // rule under the title says which screen you are on without a tab bar.
    Container::new().padding(EdgeInsets::all(20.0)).child(Flex::column().cross_axis_alignment(CrossAxisAlignment::Start).spacing(10.0)
        .push(Text::new(title).style(theme.text.title).bold())
        .push(Container::new().height(2.0).width(44.0).radius(f32::MAX).color(theme.colors.primary))
        .push(Text::new(subtitle).style(theme.text.body).color(theme.colors.on_surface_variant))).into()
}
fn navbar(theme:&ThemeData,snap:&Snapshot,handle:Option<Handle>)->WidgetNode{
    // The bar sits on its own raised surface with a hairline above it, so
    // the content scrolls *under* something rather than into nothing. The
    // active tab is the filled one and the rest are text — the framework's
    // own button hierarchy doing the work the "•" bullet used to do alone.
    let active=match &snap.route{Route::Root(t)=>Some(*t),_=>None};
    let mut row=Flex::row().spacing(6.0);
    for (tab,name) in [(Tab::Home,"Home"),(Tab::Explore,"Explore"),(Tab::Create,"Create"),(Tab::Activity,"Activity"),(Tab::Profile,"Profile")]{
        let h=handle.clone();
        let is_active=active==Some(tab);
        let mut b=Button::new(name).on_pressed(move||{if let Some(h)=&h{h.write(|s|s.tab(tab))}});
        b=if is_active{b.style(ButtonStyle::Filled)}else{b.style(ButtonStyle::Text)};
        row=row.push(Flexible::expanded(1).child(b));
    }
    Container::new()
        .color(theme.colors.surface_variant)
        .border(Border::thin(theme.colors.outline))
        .padding(EdgeInsets::symmetric(10.0,6.0))
        .child(row).into()
}

fn main()->Result<(),Box<dyn std::error::Error>>{
    let report=App::new().title("3 — social 3D").size(Size::new(480.0,860.0)).theme(ThemeData::dark()).run(|driver|driver.set_root(WidgetNode::new(SocialScreen)))?;
    println!("{report}"); Ok(())
}

// ── the receipts ─────────────────────────────────────────────────────────────
//
// The five screens of the app's story, rendered through the native
// rasteriser with no display and no window — the same tree `App::run` shows,
// mounted in the same harness the viewer's own `previews.rs` uses. Run on
// demand (ignored by default because it writes files):
//
// ```text
// cargo test -p three-social-app -- --ignored --nocapture
// ```
#[cfg(test)]
mod receipts {
    use super::*;
    use std::time::Duration;
    use vieww_test_harness::TestHarness;
    use vieww_test_harness::visual;

    const WINDOW: Size = Size { width: 480.0, height: 860.0 };
    const OUT_DIR: &str = "target/screens";

    fn mounted_app() -> TestHarness {
        let mut harness = TestHarness::new(WINDOW);
        harness.mount(Theme::new(ThemeData::dark()).child(WidgetNode::new(SocialScreen)));
        harness
    }

    /// Walk the social state the way a tap would, then let a frame happen.
    fn select(harness: &mut TestHarness, tab: Tab) {
        let cell = {
            let driver = harness.driver();
            let tree = driver.elements();
            let root = tree
                .iter()
                .into_iter()
                .find(|element| element.debug_name() == "SocialScreen")
                .expect("the root screen is mounted");
            root.state().expect("the screen owns a social state").clone()
        };
        let mut state = cell.borrow_mut();
        let social = state
            .as_any_mut()
            .downcast_mut::<SocialState>()
            .expect("the screen's state is the social state");
        social.tab(tab);
        drop(state);
        harness.request_frame();
    }

    fn save(harness: &mut TestHarness, name: &str) {
        let frame = visual::render(harness.driver(), WINDOW, Color::WHITE);
        std::fs::create_dir_all(OUT_DIR).expect("create the screens directory");
        let path = std::path::Path::new(OUT_DIR).join(name);
        frame.write_png(&path).expect("write the screen");
        println!("wrote {}", path.display());
    }

    #[test]
    #[ignore = "writes PNG files; run with -- --ignored --nocapture"]
    fn render_the_app_screens() {
        // 1. Home: the moment card, feed live, capture playing.
        let mut harness = mounted_app();
        harness.tick(Duration::from_millis(400));
        save(&mut harness, "1-home.png");

        // 2-5. One screen per tab, each settled a beat.
        for (tab, name) in [
            (Tab::Explore, "2-explore.png"),
            (Tab::Create, "3-create.png"),
            (Tab::Activity, "4-activity.png"),
            (Tab::Profile, "5-profile.png"),
        ] {
            select(&mut harness, tab);
            harness.tick(Duration::from_millis(120));
            save(&mut harness, name);
        }
    }
}
