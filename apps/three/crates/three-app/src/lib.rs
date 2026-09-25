//! Product-level application state for the `3` social network.
//!
//! UI widgets bind to this state machine; network/storage implementations sit
//! behind [`three_social::SocialBackend`]. No screen owns business logic.

use three_format::{decode, encode, FormatError};
use three_social::{Comment, FeedKind, FeedPage, Post, PostId, Privacy, Profile, PublishRequest, SocialBackend, SocialError, UploadRequest, UserId};
use three_vieww::ThreeView;

#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Tab { Home, Explore, Create, Activity, Profile }
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum HomeFeed { ForYou, Following }
#[derive(Clone, Debug, PartialEq, Eq)] pub enum Route { Root(Tab), Post(PostId), User(UserId), Comments(PostId), Composer }

#[derive(Clone, Debug, Default, PartialEq, Eq)] pub struct ComposerState { pub caption:String, pub privacy:Privacy, pub remix_of:Option<PostId>, pub asset_bytes:Option<Vec<u8>>, pub duration_ns:u64, pub title:String }
#[derive(Clone, Debug, PartialEq, Eq)] pub enum AppError { Social(SocialError), Format(String), MissingMedia }
impl From<SocialError> for AppError { fn from(v:SocialError)->Self{Self::Social(v)} }
impl From<FormatError> for AppError { fn from(v:FormatError)->Self{Self::Format(v.to_string())} }

pub struct ThreeApp<B: SocialBackend> {
    backend:B,
    pub route:Route,
    pub home_feed:HomeFeed,
    pub feed:FeedPage,
    pub composer:ComposerState,
    pub search_query:String,
    pub search_results:Vec<Profile>,
    pub last_error:Option<AppError>,
}

impl<B:SocialBackend> ThreeApp<B>{
    pub fn new(backend:B)->Self{Self{backend,route:Route::Root(Tab::Home),home_feed:HomeFeed::ForYou,feed:FeedPage::default(),composer:ComposerState::default(),search_query:String::new(),search_results:vec![],last_error:None}}
    pub fn backend(&self)->&B{&self.backend}
    pub fn backend_mut(&mut self)->&mut B{&mut self.backend}
    pub fn navigate(&mut self,route:Route){self.route=route;}
    pub fn select_tab(&mut self,tab:Tab){self.route=Route::Root(tab);}
    pub fn refresh_home(&mut self)->Result<&FeedPage,AppError>{let kind=match self.home_feed{HomeFeed::ForYou=>FeedKind::ForYou,HomeFeed::Following=>FeedKind::Following};self.feed=self.backend.feed(kind,None,None,24)?;Ok(&self.feed)}
    pub fn load_more(&mut self)->Result<&FeedPage,AppError>{let Some(cursor)=self.feed.next_cursor.clone() else{return Ok(&self.feed)};let kind=match self.home_feed{HomeFeed::ForYou=>FeedKind::ForYou,HomeFeed::Following=>FeedKind::Following};let page=self.backend.feed(kind,None,Some(&cursor),24)?;self.feed.posts.extend(page.posts);self.feed.next_cursor=page.next_cursor;Ok(&self.feed)}
    pub fn set_home_feed(&mut self,feed:HomeFeed)->Result<(),AppError>{self.home_feed=feed;self.refresh_home()?;Ok(())}
    pub fn like(&mut self,id:&PostId)->Result<bool,AppError>{let liked=self.backend.like(id)?;self.refresh_home()?;Ok(liked)}
    pub fn comment(&mut self,id:&PostId,text:String)->Result<Comment,AppError>{let c=self.backend.comment(id,text)?;self.refresh_home()?;Ok(c)}
    pub fn comments(&self,id:&PostId)->Result<Vec<Comment>,AppError>{Ok(self.backend.comments(id)?)}
    pub fn follow(&mut self,id:&UserId)->Result<bool,AppError>{let v=self.backend.follow(id)?;if self.home_feed==HomeFeed::Following{self.refresh_home()?;}Ok(v)}
    pub fn search(&mut self,q:impl Into<String>){self.search_query=q.into();self.search_results=self.backend.search_profiles(&self.search_query,30);}
    pub fn begin_create(&mut self){self.composer=ComposerState{privacy:Privacy::Public,..Default::default()};self.route=Route::Composer;}
    pub fn begin_remix(&mut self,parent:PostId){self.begin_create();self.composer.remix_of=Some(parent);}
    pub fn attach_capture(&mut self,capture:&three_core::Capture3D)->Result<(),AppError>{self.composer.asset_bytes=Some(encode(capture)?);self.composer.duration_ns=capture.duration_ns;self.composer.title=capture.title.clone();Ok(())}
    pub fn publish_composer(&mut self)->Result<Post,AppError>{let bytes=self.composer.asset_bytes.take().ok_or(AppError::MissingMedia)?;decode(&bytes)?;let asset=self.backend.upload_asset(UploadRequest{bytes,duration_ns:self.composer.duration_ns,title:self.composer.title.clone()})?;let post=self.backend.publish(PublishRequest{asset:asset.id,caption:self.composer.caption.clone(),privacy:self.composer.privacy,remix_of:self.composer.remix_of.clone()})?;self.composer=ComposerState::default();self.route=Route::Root(Tab::Home);self.refresh_home()?;Ok(post)}
    pub fn open_post_media(&self,post:&Post)->Result<ThreeView,AppError>{let bytes=self.backend.download_asset(&post.asset)?;Ok(ThreeView::new(decode(&bytes)?))}
    pub fn my_profile(&self)->Result<Profile,AppError>{let id=self.backend.active_user().ok_or(SocialError::Unauthorized)?;Ok(self.backend.profile(id)?)}
    pub fn profile_feed(&self,user:&UserId)->Result<FeedPage,AppError>{Ok(self.backend.feed(FeedKind::Profile,Some(user),None,48)?)}
}

#[cfg(test)]mod tests{use super::*;use three_runtime::demo_capture;use three_social::{demo_profile,MemorySocialBackend};
fn app()->ThreeApp<MemorySocialBackend>{ThreeApp::new(MemorySocialBackend::new(demo_profile("me","teja","Teja")))}
#[test]fn create_publish_open_media(){let mut a=app();a.begin_create();a.composer.caption="first .3".into();a.attach_capture(&demo_capture()).unwrap();let p=a.publish_composer().unwrap();assert_eq!(a.feed.posts.len(),1);let _view=a.open_post_media(&p).unwrap();}
#[test]fn navigation_is_explicit(){let mut a=app();a.select_tab(Tab::Explore);assert_eq!(a.route,Route::Root(Tab::Explore));a.begin_create();assert_eq!(a.route,Route::Composer);}
}
