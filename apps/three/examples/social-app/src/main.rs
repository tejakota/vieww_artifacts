//! `3` — reference social application shell built with Vieww.
//!
//! This is deliberately backed by the same `ThreeApp<SocialBackend>` product
//! state that a networked client uses. The bundled backend is offline-first,
//! so the app is runnable without credentials or a server.
//!
//! Every tab is a real screen in this pass. The earlier shell carried four
//! stubs — a title, an accent rule, one sentence — where Explore, Create,
//! Activity and Profile now carry people cards with wired follow buttons, the
//! capture pipeline as a timeline, the backend's own notification feed, and a
//! profile header with its moments. Nothing on any of them is a picture of a
//! control: every button routes through the product state the real client
//! would use.

use std::{cell::RefCell, collections::HashSet, fmt, rc::Rc};
use three_app::{HomeFeed, Route, Tab, ThreeApp};
use three_runtime::demo_capture;
use three_social::{
    demo_profile, MemorySocialBackend, Notification, NotificationKind, Post, PostId, Profile,
    SocialBackend, UserId,
};
use vieww::prelude::*;
use vieww::widget::ElementState;
use vieww_platform_winit::App;
use vieww_integration::ViewerScreen;

// ── the glyphs this shell draws ─────────────────────────────────────────────
//
// The framework's icon set is deliberately six shapes; the social shell needs
// a few more, drawn on the same 24×24 grid with the same filled-outline
// convention `vieww_widget::icons` documents.
mod icons {
    use vieww::foundation::{IconData, Offset, Path};

    /// The circle-to-Bézier constant for hand-drawn arcs.
    const KAPPA: f32 = 0.552_284_75;

    fn circle(path: &mut Path, cx: f32, cy: f32, r: f32, clockwise: bool) {
        let k = KAPPA * r;
        let s = if clockwise { 1.0 } else { -1.0 };
        path.move_to(Offset::new(cx + r, cy));
        path.cubic_to(
            Offset::new(cx + r, cy + s * k),
            Offset::new(cx + k, cy + s * r),
            Offset::new(cx, cy + s * r),
        );
        path.cubic_to(
            Offset::new(cx - k, cy + s * r),
            Offset::new(cx - r, cy + s * k),
            Offset::new(cx - r, cy),
        );
        path.cubic_to(
            Offset::new(cx - r, cy - s * k),
            Offset::new(cx - k, cy - s * r),
            Offset::new(cx, cy - s * r),
        );
        path.cubic_to(
            Offset::new(cx + k, cy - s * r),
            Offset::new(cx + r, cy - s * k),
            Offset::new(cx + r, cy),
        );
        path.close();
    }

    /// A heart, filled — the like. Four cubics: bottom point up the left
    /// side, around the left lobe, across the dip, around the right lobe, and
    /// home — each with two control points, as `cubic_to` takes them.
    pub fn heart() -> IconData {
        let mut path = Path::new();
        path.move_to(Offset::new(12.0, 20.4));
        path.cubic_to(
            Offset::new(4.2, 15.2),
            Offset::new(2.4, 9.6),
            Offset::new(5.6, 6.6),
        );
        path.cubic_to(
            Offset::new(8.2, 4.6),
            Offset::new(10.6, 5.8),
            Offset::new(12.0, 8.0),
        );
        path.cubic_to(
            Offset::new(13.4, 5.8),
            Offset::new(15.8, 4.6),
            Offset::new(18.4, 6.6),
        );
        path.cubic_to(
            Offset::new(21.6, 9.6),
            Offset::new(19.8, 15.2),
            Offset::new(12.0, 20.4),
        );
        path.close();
        IconData::square24(path)
    }

    /// A head and shoulders — a person.
    pub fn person() -> IconData {
        let mut path = Path::new();
        circle(&mut path, 12.0, 7.6, 3.9, true);
        // The shoulders: a wide shallow dome off the bottom of the grid.
        path.move_to(Offset::new(3.6, 20.4));
        path.cubic_to(
            Offset::new(3.6, 14.9),
            Offset::new(7.4, 12.4),
            Offset::new(12.0, 12.4),
        );
        path.cubic_to(
            Offset::new(16.6, 12.4),
            Offset::new(20.4, 14.9),
            Offset::new(20.4, 20.4),
        );
        path.close();
        IconData::square24(path)
    }

    /// A magnifier — search.
    pub fn search() -> IconData {
        let mut path = Path::new();
        circle(&mut path, 10.4, 10.4, 6.8, true);
        circle(&mut path, 10.4, 10.4, 4.9, false);
        let (x0, y0) = (15.0, 15.0);
        let (x1, y1) = (20.4, 20.4);
        let half = 1.2;
        path.move_to(Offset::new(x0 - half, y0 + half));
        path.line_to(Offset::new(x1 - half, y1 + half));
        path.line_to(Offset::new(x1 + half, y1 - half));
        path.line_to(Offset::new(x0 + half, y0 - half));
        path.close();
        IconData::square24(path)
    }

    /// A speech bubble — a comment. A rounded body plus a tail, both filled,
    /// overlapping: the nonzero rule takes their union.
    pub fn comment() -> IconData {
        let mut path = Path::new();
        path.extend(&Path::rounded_rect(
            vieww::foundation::Rect::new(2.6, 4.8, 21.4, 16.6),
            5.2,
        ));
        path.move_to(Offset::new(8.2, 15.8));
        path.line_to(Offset::new(11.8, 21.0));
        path.line_to(Offset::new(13.4, 15.8));
        path.close();
        IconData::square24(path)
    }

    /// A cube in three-quarter view — the `.3` format's own mark.
    pub fn cube() -> IconData {
        let mut path = Path::new();
        // The top face, the front face, and the right face — one silhouette
        // with two internal seams left as gaps, which is how an unfilled
        // wireframe has to spell itself when the painter only fills.
        let top = [
            (12.0, 3.0),
            (20.4, 7.6),
            (12.0, 12.2),
            (3.6, 7.6),
        ];
        let front = [
            (3.6, 7.6),
            (12.0, 12.2),
            (12.0, 21.0),
            (3.6, 16.4),
        ];
        let right = [
            (20.4, 7.6),
            (12.0, 12.2),
            (12.0, 21.0),
            (20.4, 16.4),
        ];
        for face in [top, front, right] {
            path.move_to(Offset::new(face[0].0, face[0].1));
            for &(x, y) in &face[1..] {
                path.line_to(Offset::new(x, y));
            }
            path.close();
        }
        IconData::square24(path)
    }

}

/// The seeded world: three people, one published moment, and a real
/// notification trail left behind by the other two acting on it — every like,
/// comment and follow in the Activity screen happened through the backend's
/// own API, as the people it says did them.
fn seeded_app() -> ThreeApp<MemorySocialBackend> {
    let mut teja = demo_profile("teja", "teja", "Teja");
    teja.bio = "Collecting moments, not photos. Circling the table since day one.".into();
    teja.posts = 1;
    let mut maya = demo_profile("maya", "maya3d", "Maya");
    maya.bio = "Sculptor. I remix other people's light.".into();
    maya.followers = 214;
    maya.following = 86;
    let mut noah = demo_profile("noah", "noah.space", "Noah");
    noah.bio = "Orbits, mostly.".into();
    noah.followers = 97;
    noah.following = 143;

    let mut backend = MemorySocialBackend::new(teja);
    backend.add_profile(maya);
    backend.add_profile(noah);
    let mut app = ThreeApp::new(backend);
    app.begin_create();
    app.composer.caption = "A living 3D moment — drag it, rotate it, make it yours.".into();
    app.attach_capture(&demo_capture()).expect("demo capture encodes");
    let post = app.publish_composer().expect("seed post publishes");

    // The trail, left by real actors through the real actions. `set_active_user`
    // is the whole trick: the backend attributes every act to whoever is
    // active, exactly as a multi-user server would.
    let teja_id = UserId("teja".into());
    {
        let backend = app.backend_mut();
        let _ = backend.set_active_user(UserId("maya".into()));
        let _ = backend.like(&post.id);
        let _ = backend.follow(&teja_id);
        let _ = backend.set_active_user(UserId("noah".into()));
        let _ = backend.like(&post.id);
        let _ = backend.comment(&post.id, "The orbit read on this one is perfect.".into());
        let _ = backend.follow(&teja_id);
        let _ = backend.set_active_user(teja_id.clone());
    }
    app.refresh_home().expect("the feed refreshes");
    app
}

/// One row of the Activity feed, resolved for drawing: who did what, and the
/// glyph that says which kind of act it was.
#[derive(Clone, Debug)]
struct ActivityRow {
    actor: String,
    actor_handle: String,
    verb: String,
    unread: bool,
    kind: NotificationKind,
}

struct SocialState {
    app: ThreeApp<MemorySocialBackend>,
    pending: bool,
    /// The Explore search field's live text.
    search: String,
    /// Who the signed-in user follows, mirrored from the backend's boolean
    /// answers so the buttons can say what they do. The backend keeps its own
    /// set privately; this is the view's copy, written only by the same call
    /// that writes the backend's.
    following: HashSet<UserId>,
}
impl fmt::Debug for SocialState { fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result{f.debug_struct("SocialState").field("route",&self.app.route).field("feed_items",&self.app.feed.posts.len()).finish()} }
impl SocialState {
    fn new()->Self{Self{app:seeded_app(),pending:false,search:String::new(),following:HashSet::new()}}
    fn tab(&mut self,tab:Tab){self.app.select_tab(tab);self.pending=true;}
    fn toggle_feed(&mut self){let next=if self.app.home_feed==HomeFeed::ForYou{HomeFeed::Following}else{HomeFeed::ForYou};let _=self.app.set_home_feed(next);self.pending=true;}
    fn like(&mut self,id:&PostId){let _=self.app.like(id);self.pending=true;}
    fn set_search(&mut self,text:String){self.search=text;self.pending=true;}
    /// Follow or unfollow through the product state, keeping the view's mirror
    /// in step with the boolean the backend returns.
    fn follow(&mut self,id:&UserId){
        match self.app.follow(id) {
            Ok(true) => { self.following.insert(id.clone()); }
            Ok(false) => { self.following.remove(id); }
            Err(_) => {}
        }
        self.pending=true;
    }
    fn begin_create(&mut self){self.app.begin_create();self.pending=true;}
    fn discard_composer(&mut self){self.app.select_tab(Tab::Create);self.pending=true;}
}
impl ElementState for SocialState {
    fn as_any(&self)->&dyn std::any::Any{self}
    fn as_any_mut(&mut self)->&mut dyn std::any::Any{self}
    fn take_pending(&mut self)->bool{std::mem::take(&mut self.pending)}
}
#[derive(Clone)] struct Handle(Rc<RefCell<dyn ElementState>>);
impl Handle { fn write(&self,f:impl FnOnce(&mut SocialState)){let mut s=self.0.borrow_mut();if let Some(s)=s.as_any_mut().downcast_mut::<SocialState>(){f(s)}} }
impl fmt::Debug for Handle { fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result{f.write_str("Handle(<SocialState>)")} }

#[derive(Clone)]
struct Snapshot {
    route: Route,
    feed: HomeFeed,
    posts: Vec<Post>,
    /// Everyone the backend knows, self included — Explore filters the self
    /// out at draw time so the list is what the search actually returns.
    people: Vec<Profile>,
    me: Profile,
    activity: Vec<ActivityRow>,
    following: Vec<UserId>,
    search: String,
}
impl Snapshot {
    fn read(s:&SocialState)->Self{
        let people=s.app.backend().search_profiles("",30);
        let me=people.iter().find(|p|p.id.0=="teja").cloned()
            .unwrap_or_else(||demo_profile("teja","teja","Teja"));
        let name_of=|id:&UserId|people.iter().find(|p|&p.id==id);
        let activity=s.app.backend().notifications()
            .expect("the backend serves its notifications")
            .into_iter()
            .map(|Notification{actor,kind,read,..}|{
                let who=name_of(&actor);
                let verb=match &kind{
                    NotificationKind::Like{..}=>"liked your moment",
                    NotificationKind::Comment{..}=>"commented on your moment",
                    NotificationKind::Follow{..}=>"started following you",
                    NotificationKind::Remix{..}=>"remixed your moment",
                }.to_string();
                ActivityRow{
                    actor:who.map(|p|p.display_name.clone()).unwrap_or_else(||"Someone".into()),
                    actor_handle:who.map(|p|format!("@{}",p.handle)).unwrap_or_default(),
                    verb,
                    unread:!read,
                    kind,
                }
            })
            .collect();
        Self{
            route:s.app.route.clone(),
            feed:s.app.home_feed,
            posts:s.app.feed.posts.clone(),
            people,
            me,
            activity,
            following:s.following.iter().cloned().collect(),
            search:s.search.clone(),
        }
    }
    fn initial()->Self{Self{route:Route::Root(Tab::Home),feed:HomeFeed::ForYou,posts:vec![],people:vec![],me:demo_profile("teja","teja","Teja"),activity:vec![],following:vec![],search:String::new()}}
}

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
            Route::Root(Tab::Explore)=>explore_screen(&theme,&snap,handle.clone()),
            Route::Root(Tab::Create)=>create_screen(&theme,false,handle.clone()),
            Route::Composer=>create_screen(&theme,true,handle.clone()),
            Route::Root(Tab::Activity)=>activity_screen(&theme,&snap),
            Route::Root(Tab::Profile)=>profile_screen(&theme,&snap),
            _=>simple_screen(&theme,"3","Detail route"),
        };
        Container::new().color(theme.colors.surface).child(
            Flex::column().cross_axis_alignment(CrossAxisAlignment::Stretch)
                .push(Flexible::expanded(1).child(body))
                .push(navbar(&theme,&snap,handle))
        )
    }
}

// ── the screens ─────────────────────────────────────────────────────────────

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

/// Explore: a live search field over the backend's own people, each card
/// carrying a Follow button wired through the product state. The list the
/// field filters is `search_profiles`' actual answer — the same call the
/// networked client makes.
fn explore_screen(theme:&ThemeData,snap:&Snapshot,handle:Option<Handle>)->WidgetNode{
    let on_search=handle.clone();
    let field=Container::new()
        .color(theme.colors.surface_variant)
        .radius(f32::MAX)
        .padding(EdgeInsets::symmetric(6.0,4.0))
        .child(
            Flex::row().cross_axis_alignment(CrossAxisAlignment::Center).spacing(8.0)
                .push(Icon::new(icons::search()).size(17.0).color(theme.colors.on_surface_variant))
                .push(Flexible::expanded(1).child(
                    TextField::text(snap.search.clone())
                        .placeholder("Search people")
                        .placeholder_color(theme.colors.on_surface_variant)
                        .color(theme.colors.on_surface)
                        .cursor(theme.colors.primary,2.0)
                        .single_line()
                        .on_changed(Rc::new(move|value:TextEditingValue|{
                            if let Some(h)=&on_search{h.write(|s|s.set_search(value.text));}
                        })),
                )),
        );

    let q=snap.search.trim().to_lowercase();
    let people:Vec<&Profile>=snap.people.iter()
        .filter(|p|p.id.0!="teja")
        .filter(|p|q.is_empty()||p.handle.to_lowercase().contains(&q)||p.display_name.to_lowercase().contains(&q))
        .collect();

    let mut body:Vec<WidgetNode>=vec![Text::new("People").style(theme.text.title).bold().into()];
    if people.is_empty(){
        body.push(Text::new("No one by that name yet.").style(theme.text.body).color(theme.colors.on_surface_variant).into());
    }
    for person in people{
        let follow=handle.clone();
        let id=person.id.clone();
        let is_following=snap.following.iter().any(|f|f==&id);
        let initials: String=person.display_name.split_whitespace().take(2)
            .filter_map(|w|w.chars().next()).flat_map(char::to_uppercase).collect();
        body.push(person_card(theme,person,&initials,is_following,move||{
            if let Some(h)=&follow{h.write(|s|s.follow(&id));}
        }));
    }

    Container::new().padding(EdgeInsets::all(20.0)).child(
        Flex::column().cross_axis_alignment(CrossAxisAlignment::Stretch).spacing(12.0)
            .push(field)
            .push(Flexible::expanded(1).child(
                Scrollable::vertical(0.0).child(
                    Flex::column().cross_axis_alignment(CrossAxisAlignment::Stretch).spacing(10.0)
                        .children(body),
                ),
            )),
    ).into()
}

/// One person in Explore: an avatar disc carrying the person's own accent,
/// their name and handle, and the follow control.
fn person_card(theme:&ThemeData,person:&Profile,initials:&str,following:bool,on_follow:impl Fn()+'static)->WidgetNode{
    let avatar=Container::new()
        .size(46.0,46.0)
        .radius(f32::MAX)
        .gradient(accent_for(&person.handle))
        .alignment(Alignment::CENTER)
        .child(Text::new(initials.to_string()).color(Color::WHITE).size(15.0).bold());

    let names=Flex::column().cross_axis_alignment(CrossAxisAlignment::Start).spacing(1.0)
        .push(Text::new(person.display_name.clone()).style(theme.text.body).bold())
        .push(Text::new(format!("{} · {} followers",person.handle,person.followers)).style(theme.text.label).color(theme.colors.on_surface_variant));

    let button=if following{
        Button::new("Following").on_pressed(move||on_follow())
    }else{
        Button::new("Follow").style(ButtonStyle::Filled).on_pressed(move||on_follow())
    };

    Container::new()
        .color(theme.colors.surface_variant)
        .radius(16.0)
        .padding(EdgeInsets::symmetric(12.0,10.0))
        .child(Flex::row().cross_axis_alignment(CrossAxisAlignment::Center).spacing(12.0)
            .push(avatar)
            .push(Flexible::expanded(1).child(names))
            .push(button))
        .into()
}

/// A stable per-person accent: the theme's primary hue rotated by a hash of
/// the handle, so a feed of people is a feed of distinguishable people
/// without inventing a colour system the theme does not carry.
fn accent_for(handle:&str)->Gradient{
    let hash=handle.bytes().fold(0x811c9dc5u32,|h,b|((h^b as u32).wrapping_mul(0x01000193))&0xffff_ffff);
    let hue=(hash%360) as f32;
    let top=hsl(hue,0.62,0.62);
    let bottom=hsl(hue+34.0,0.66,0.42);
    Gradient::linear(Offset::new(0.5,0.0),Offset::new(0.5,1.0))
        .with_stops(&[(0.0,top),(1.0,bottom)])
}

/// HSL → RGB, the one colour helper this shell needs. The framework's `Color`
/// takes components; it does not take a wheel position.
fn hsl(hue:f32,saturation:f32,lightness:f32)->Color{
    let h=((hue%360.0)+360.0)%360.0/360.0;
    let s=saturation.clamp(0.0,1.0);
    let l=lightness.clamp(0.0,1.0);
    let c=(1.0-(2.0*l-1.0).abs())*s;
    let x=c*(1.0-((h*6.0)%2.0-1.0).abs());
    let m=l-c*0.5;
    let (r,g,b)=match (h*6.0) as u32{
        0=>(c,x,0.0),1=>(x,c,0.0),2=>(0.0,c,x),
        3=>(0.0,x,c),4=>(x,0.0,c),_=>(c,0.0,x),
    };
    Color::rgb(((r+m)*255.0) as u8,((g+m)*255.0) as u8,((b+m)*255.0) as u8)
}

/// Create: the pipeline as a timeline — six numbered stages, joined by a
/// spine, the first active when a capture is actually in flight.
fn create_screen(theme:&ThemeData,capturing:bool,handle:Option<Handle>)->WidgetNode{
    const STEPS:[(&str,&str);6]=[
        ("Capture","A slow orbit around the subject — the phone is the scanner."),
        ("Reconstruct","Frames become a mesh per instant, in the .3 container."),
        ("Preview","Turn it, light it, decide it is worth keeping."),
        ("Caption","Say what the moment was, not what the file is."),
        ("Privacy","Public, followers, or private — per moment, forever."),
        ("Publish","It lands in the feed as a moment, not as a photo."),
    ];

    let mut rows:Vec<WidgetNode>=vec![Text::new("Create").style(theme.text.title).bold().into()];
    for (index,(name,blurb)) in STEPS.iter().enumerate(){
        let active=capturing&&index==0;
        let disc=Container::new()
            .size(30.0,30.0)
            .radius(f32::MAX)
            .color(if active{theme.colors.primary}else{theme.colors.surface_variant})
            .alignment(Alignment::CENTER)
            .child(Text::new((index+1).to_string()).size(13.0).bold()
                .color(if active{theme.colors.on_primary}else{theme.colors.on_surface_variant}));
        let row=Flex::row().cross_axis_alignment(CrossAxisAlignment::Center).spacing(12.0)
            .push(disc)
            .push(Flexible::expanded(1).child(
                Flex::column().cross_axis_alignment(CrossAxisAlignment::Start).spacing(1.0)
                    .push(Text::new(name.to_string()).style(theme.text.body).bold()
                        .color(if active{theme.colors.on_surface}else{theme.colors.on_surface_variant}))
                    .push(Text::new(blurb.to_string()).style(theme.text.label).color(theme.colors.on_surface_variant)),
            ));
        rows.push(Container::new().padding(EdgeInsets::only(0.0,6.0,0.0,6.0)).child(row).into());
    }

    let begin=handle.clone();
    let action=if capturing{
        Button::new("Discard this capture").on_pressed(move||{if let Some(h)=&begin{h.write(|s|s.discard_composer());}})
    }else{
        Button::new("Begin a capture").style(ButtonStyle::Filled).on_pressed(move||{if let Some(h)=&begin{h.write(|s|s.begin_create());}})
    };

    Container::new().padding(EdgeInsets::all(20.0)).child(
        Flex::column().cross_axis_alignment(CrossAxisAlignment::Stretch).spacing(10.0)
            .push(Text::new(if capturing{"Capture in flight"}else{"A moment, start to finish"}).style(theme.text.label).color(theme.colors.primary).bold())
            .push(Flexible::expanded(1).child(
                Scrollable::vertical(0.0).child(
                    Flex::column().cross_axis_alignment(CrossAxisAlignment::Stretch).spacing(6.0)
                        .children(rows),
                ),
            ))
            .push(action),
    ).into()
}

/// Activity: the backend's own notification feed, each act with its glyph and
/// actor. Every row happened — see `seeded_app` for the acts that left them.
fn activity_screen(theme:&ThemeData,snap:&Snapshot)->WidgetNode{
    let mut rows:Vec<WidgetNode>=vec![Text::new("Activity").style(theme.text.title).bold().into()];

    if snap.activity.is_empty(){
        rows.push(Text::new("Likes, comments, follows and remixes land here — the moment you publish, so does the first one.")
            .style(theme.text.body).color(theme.colors.on_surface_variant).into());
    }
    for item in &snap.activity{
        let (glyph,tint)=match item.kind{
            NotificationKind::Like{..}=>(icons::heart(),theme.colors.primary),
            NotificationKind::Comment{..}=>(icons::comment(),theme.colors.primary),
            NotificationKind::Follow{..}=>(icons::person(),theme.colors.primary),
            NotificationKind::Remix{..}=>(icons::cube(),theme.colors.primary),
        };
        let disc=Container::new()
            .size(36.0,36.0)
            .radius(f32::MAX)
            .color(theme.colors.surface_variant)
            .alignment(Alignment::CENTER)
            .child(Icon::new(glyph).size(16.0).color(tint));

        let mut line=Flex::row().cross_axis_alignment(CrossAxisAlignment::Center).spacing(10.0)
            .push(disc)
            .push(Flexible::expanded(1).child(
                Flex::column().cross_axis_alignment(CrossAxisAlignment::Start).spacing(1.0)
                    .push(Flex::row().cross_axis_alignment(CrossAxisAlignment::Baseline).spacing(6.0)
                        .push(Text::new(item.actor.clone()).style(theme.text.body).bold())
                        .push(Text::new(item.actor_handle.clone()).style(theme.text.label).color(theme.colors.on_surface_variant)))
                    .push(Text::new(item.verb.clone()).style(theme.text.label).color(theme.colors.on_surface_variant)),
            ));
        if item.unread{
            line=line.push(Container::new().size(8.0,8.0).radius(f32::MAX).color(theme.colors.primary));
        }
        rows.push(
            Container::new().color(theme.colors.surface_variant).radius(16.0)
                .padding(EdgeInsets::symmetric(12.0,10.0))
                .child(line).into(),
        );
    }

    Container::new().padding(EdgeInsets::all(20.0)).child(
        Scrollable::vertical(0.0).child(
            Flex::column().cross_axis_alignment(CrossAxisAlignment::Stretch).spacing(10.0)
                .children(rows),
        ),
    ).into()
}

/// Profile: the signed-in person, their numbers, and their moments as cards.
fn profile_screen(theme:&ThemeData,snap:&Snapshot)->WidgetNode{
    let me=&snap.me;

    let ring=Container::new()
        .size(84.0,84.0)
        .radius(f32::MAX)
        .gradient(Gradient::linear(Offset::new(0.0,0.0),Offset::new(1.0,1.0)).with_stops(&[
            (0.0,theme.colors.primary),
            (1.0,hsl(210.0,0.7,0.6)),
        ]))
        .padding(EdgeInsets::all(3.0))
        .child(Container::new().color(theme.colors.surface).radius(f32::MAX).padding(EdgeInsets::all(3.0))
            .child(Container::new().size(72.0,72.0).radius(f32::MAX)
                .color(theme.colors.surface_variant)
                .alignment(Alignment::CENTER)
                .child(Text::new("T".to_string()).size(28.0).bold().color(theme.colors.on_surface))));

    let stat=|label:&str,value:&str|{
        Flex::column().cross_axis_alignment(CrossAxisAlignment::Center).spacing(2.0)
            .push(Text::new(value.to_string()).size(19.0).bold().color(theme.colors.on_surface))
            .push(Text::new(label.to_string()).style(theme.text.label).color(theme.colors.on_surface_variant))
    };

    let header=Flex::column().cross_axis_alignment(CrossAxisAlignment::Stretch).spacing(12.0)
        .push(Flex::row().cross_axis_alignment(CrossAxisAlignment::Center).spacing(14.0)
            .push(ring)
            .push(Flexible::expanded(1).child(
                Flex::column().cross_axis_alignment(CrossAxisAlignment::Start).spacing(3.0)
                    .push(Text::new(me.display_name.clone()).style(theme.text.title).bold())
                    .push(Text::new(format!("@{}",me.handle)).style(theme.text.label).color(theme.colors.primary))
                    .push(Text::new(me.bio.clone()).style(theme.text.body).color(theme.colors.on_surface_variant)),
            )))
        .push(Container::new().color(theme.colors.surface_variant).radius(16.0)
            .padding(EdgeInsets::symmetric(8.0,12.0))
            .child(Flex::row().main_axis_alignment(MainAxisAlignment::SpaceAround)
                .push(stat("moments",&me.posts.to_string()))
                .push(stat("followers",&me.followers.to_string()))
                .push(stat("following",&me.following.to_string()))));

    let mut moments:Vec<WidgetNode>=vec![Text::new("Moments").style(theme.text.title).bold().into()];
    for post in &snap.posts{
        moments.push(
            Container::new().color(theme.colors.surface_variant).radius(16.0)
                .padding(EdgeInsets::all(12.0))
                .child(
                    Flex::column().cross_axis_alignment(CrossAxisAlignment::Stretch).spacing(8.0)
                        .push(Flex::row().cross_axis_alignment(CrossAxisAlignment::Center).spacing(8.0)
                            .push(Icon::new(icons::cube()).size(15.0).color(theme.colors.primary))
                            .push(Text::new("3.0s · .3 moment".to_string()).style(theme.text.label).color(theme.colors.on_surface_variant))
                            .push(Flexible::expanded(1).child(SizedBox::new()))
                            .push(Icon::new(icons::heart()).size(13.0).color(theme.colors.on_surface_variant))
                            .push(Text::new(post.likes.to_string()).style(theme.text.label).color(theme.colors.on_surface_variant))
                            .push(Icon::new(icons::comment()).size(13.0).color(theme.colors.on_surface_variant))
                            .push(Text::new(post.comments.to_string()).style(theme.text.label).color(theme.colors.on_surface_variant)))
                        .push(Text::new(post.caption.clone()).style(theme.text.body))
                ).into(),
        );
    }
    if snap.posts.is_empty(){
        moments.push(Text::new("Nothing published yet. The first orbit is one tap away.")
            .style(theme.text.body).color(theme.colors.on_surface_variant).into());
    }

    Container::new().padding(EdgeInsets::all(20.0)).child(
        Scrollable::vertical(0.0).child(
            Flex::column().cross_axis_alignment(CrossAxisAlignment::Stretch).spacing(12.0)
                .push(header)
                .children(moments),
        ),
    ).into()
}

fn simple_screen(theme:&ThemeData,title:&str,subtitle:&str)->WidgetNode{
    // Kept for the detail routes the shell does not draw yet — a title, a
    // rule, a line, rather than a blank surface.
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
        return harness;
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
