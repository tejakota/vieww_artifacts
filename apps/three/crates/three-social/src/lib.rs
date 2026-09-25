//! Social domain and an executable offline-first service for the `3` network.
//!
//! The domain stays transport-neutral: the same [`SocialBackend`] can sit behind
//! an HTTP server, a device-local cache, or tests. `MemorySocialBackend` is a
//! complete deterministic implementation used by the reference application.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)] pub struct UserId(pub String);
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)] pub struct AssetId(pub String);
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)] pub struct PostId(pub String);
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)] pub struct CommentId(pub String);

#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Privacy { Public, Followers, Private }
impl Default for Privacy { fn default() -> Self { Self::Public } }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    pub id: UserId,
    pub handle: String,
    pub display_name: String,
    pub bio: String,
    pub followers: u64,
    pub following: u64,
    pub posts: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetMeta {
    pub id: AssetId,
    pub owner: UserId,
    pub byte_len: u64,
    pub duration_ns: u64,
    pub title: String,
    pub created_at: SystemTime,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Post {
    pub id: PostId,
    pub author: UserId,
    pub asset: AssetId,
    pub caption: String,
    pub created_at: SystemTime,
    pub likes: u64,
    pub comments: u64,
    pub privacy: Privacy,
    pub remix_of: Option<PostId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Comment {
    pub id: CommentId,
    pub post: PostId,
    pub author: UserId,
    pub text: String,
    pub created_at: SystemTime,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotificationKind { Like { post: PostId }, Comment { post: PostId }, Follow { user: UserId }, Remix { post: PostId } }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notification { pub id: String, pub recipient: UserId, pub actor: UserId, pub kind: NotificationKind, pub created_at: SystemTime, pub read: bool }

#[derive(Clone, Debug, Default, PartialEq, Eq)] pub struct FeedPage { pub posts: Vec<Post>, pub next_cursor: Option<String> }
#[derive(Clone, Debug, PartialEq, Eq)] pub struct PublishRequest { pub asset: AssetId, pub caption: String, pub privacy: Privacy, pub remix_of: Option<PostId> }
#[derive(Clone, Debug, PartialEq, Eq)] pub struct UploadRequest { pub bytes: Vec<u8>, pub duration_ns: u64, pub title: String }

#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum FeedKind { ForYou, Following, Profile }

pub trait SocialBackend {
    fn active_user(&self) -> Option<&UserId>;
    fn profile(&self, user: &UserId) -> Result<Profile, SocialError>;
    fn update_profile(&mut self, display_name: String, bio: String) -> Result<Profile, SocialError>;
    fn upload_asset(&mut self, request: UploadRequest) -> Result<AssetMeta, SocialError>;
    fn download_asset(&self, asset: &AssetId) -> Result<Vec<u8>, SocialError>;
    fn publish(&mut self, request: PublishRequest) -> Result<Post, SocialError>;
    fn delete_post(&mut self, post: &PostId) -> Result<(), SocialError>;
    fn feed(&self, kind: FeedKind, owner: Option<&UserId>, cursor: Option<&str>, limit: usize) -> Result<FeedPage, SocialError>;
    fn like(&mut self, post: &PostId) -> Result<bool, SocialError>;
    fn comment(&mut self, post: &PostId, text: String) -> Result<Comment, SocialError>;
    fn comments(&self, post: &PostId) -> Result<Vec<Comment>, SocialError>;
    fn follow(&mut self, user: &UserId) -> Result<bool, SocialError>;
    fn search_profiles(&self, query: &str, limit: usize) -> Vec<Profile>;
    fn notifications(&self) -> Result<Vec<Notification>, SocialError>;
}

#[derive(Clone)] struct StoredAsset { meta: AssetMeta, bytes: Vec<u8> }

/// Deterministic, dependency-free social backend. It implements real social
/// semantics and is suitable for the app's offline mode and integration tests.
pub struct MemorySocialBackend {
    me: UserId,
    users: BTreeMap<UserId, Profile>,
    assets: BTreeMap<AssetId, StoredAsset>,
    posts: BTreeMap<PostId, Post>,
    post_order: Vec<PostId>,
    comments: BTreeMap<PostId, Vec<Comment>>,
    likes: BTreeSet<(UserId, PostId)>,
    follows: BTreeSet<(UserId, UserId)>,
    notifications: Vec<Notification>,
    sequence: u64,
    epoch_ms: u64,
}

impl MemorySocialBackend {
    pub fn new(me: Profile) -> Self {
        let id = me.id.clone();
        let mut users = BTreeMap::new(); users.insert(id.clone(), me);
        Self { me: id, users, assets: BTreeMap::new(), posts: BTreeMap::new(), post_order: vec![], comments: BTreeMap::new(), likes: BTreeSet::new(), follows: BTreeSet::new(), notifications: vec![], sequence: 0, epoch_ms: 1_700_000_000_000 }
    }
    pub fn add_profile(&mut self, profile: Profile) { self.users.insert(profile.id.clone(), profile); }
    pub fn set_active_user(&mut self, id: UserId) -> Result<(), SocialError> { if self.users.contains_key(&id) { self.me=id; Ok(()) } else { Err(SocialError::NotFound("user".into())) } }
    fn next_id(&mut self, prefix: &str) -> String { self.sequence += 1; format!("{prefix}-{:08}", self.sequence) }
    fn now(&mut self) -> SystemTime { self.epoch_ms += 1; UNIX_EPOCH + Duration::from_millis(self.epoch_ms) }
    fn actor(&self) -> Result<UserId, SocialError> { if self.users.contains_key(&self.me) { Ok(self.me.clone()) } else { Err(SocialError::Unauthorized) } }
    fn can_view(&self, p: &Post) -> bool { p.author == self.me || match p.privacy { Privacy::Public => true, Privacy::Private => false, Privacy::Followers => self.follows.contains(&(self.me.clone(), p.author.clone())) } }
    fn notify(&mut self, recipient: UserId, actor: UserId, kind: NotificationKind) { if recipient == actor { return; } let id=self.next_id("notification"); let created_at=self.now(); self.notifications.push(Notification{id,recipient,actor,kind,created_at,read:false}); }
}

impl SocialBackend for MemorySocialBackend {
    fn active_user(&self)->Option<&UserId>{Some(&self.me)}
    fn profile(&self,user:&UserId)->Result<Profile,SocialError>{self.users.get(user).cloned().ok_or_else(||SocialError::NotFound("user".into()))}
    fn update_profile(&mut self,display_name:String,bio:String)->Result<Profile,SocialError>{validate_text("display name",&display_name,1,80)?;validate_text("bio",&bio,0,240)?;let p=self.users.get_mut(&self.me).ok_or(SocialError::Unauthorized)?;p.display_name=display_name;p.bio=bio;Ok(p.clone())}
    fn upload_asset(&mut self,request:UploadRequest)->Result<AssetMeta,SocialError>{if request.bytes.is_empty(){return Err(SocialError::InvalidInput("asset is empty".into()));} if request.bytes.len()>256*1024*1024{return Err(SocialError::InvalidInput("asset exceeds 256 MiB".into()));} let owner=self.actor()?;let id=AssetId(self.next_id("asset"));let meta=AssetMeta{id:id.clone(),owner,byte_len:request.bytes.len() as u64,duration_ns:request.duration_ns,title:request.title,created_at:self.now()};self.assets.insert(id,StoredAsset{meta:meta.clone(),bytes:request.bytes});Ok(meta)}
    fn download_asset(&self,asset:&AssetId)->Result<Vec<u8>,SocialError>{self.assets.get(asset).map(|a|a.bytes.clone()).ok_or_else(||SocialError::NotFound("asset".into()))}
    fn publish(&mut self,request:PublishRequest)->Result<Post,SocialError>{validate_text("caption",&request.caption,0,2200)?;let author=self.actor()?;let asset=self.assets.get(&request.asset).ok_or_else(||SocialError::NotFound("asset".into()))?;if asset.meta.owner!=author{return Err(SocialError::Forbidden);}if let Some(parent)=&request.remix_of{if !self.posts.contains_key(parent){return Err(SocialError::NotFound("remix source".into()));}}let id=PostId(self.next_id("post"));let post=Post{id:id.clone(),author:author.clone(),asset:request.asset,caption:request.caption,created_at:self.now(),likes:0,comments:0,privacy:request.privacy,remix_of:request.remix_of.clone()};self.posts.insert(id.clone(),post.clone());self.post_order.push(id);if let Some(parent)=request.remix_of{let owner=self.posts.get(&parent).map(|p|p.author.clone());if let Some(owner)=owner{self.notify(owner,author,NotificationKind::Remix{post:parent});}}if let Some(profile)=self.users.get_mut(&self.me){profile.posts+=1;}Ok(post)}
    fn delete_post(&mut self,post:&PostId)->Result<(),SocialError>{let p=self.posts.get(post).ok_or_else(||SocialError::NotFound("post".into()))?;if p.author!=self.me{return Err(SocialError::Forbidden);}self.posts.remove(post);self.post_order.retain(|x|x!=post);self.comments.remove(post);self.likes.retain(|(_,p)|p!=post);if let Some(profile)=self.users.get_mut(&self.me){profile.posts=profile.posts.saturating_sub(1);}Ok(())}
    fn feed(&self,kind:FeedKind,owner:Option<&UserId>,cursor:Option<&str>,limit:usize)->Result<FeedPage,SocialError>{let start=cursor.and_then(|c|c.parse::<usize>().ok()).unwrap_or(0);let mut candidates:Vec<Post>=self.post_order.iter().rev().filter_map(|id|self.posts.get(id)).filter(|p|self.can_view(p)).filter(|p|match kind{FeedKind::ForYou=>true,FeedKind::Following=>p.author==self.me||self.follows.contains(&(self.me.clone(),p.author.clone())),FeedKind::Profile=>owner.map(|u|u==&p.author).unwrap_or(false)}).cloned().collect();if kind==FeedKind::ForYou{candidates.sort_by_key(|p|std::cmp::Reverse((p.likes*3+p.comments*5,ms(p.created_at))));}let end=(start+limit.max(1)).min(candidates.len());let posts=if start<candidates.len(){candidates[start..end].to_vec()}else{vec![]};Ok(FeedPage{posts,next_cursor:(end<candidates.len()).then(||end.to_string())})}
    fn like(&mut self,post:&PostId)->Result<bool,SocialError>{let actor=self.actor()?;let author=self.posts.get(post).ok_or_else(||SocialError::NotFound("post".into()))?.author.clone();let key=(actor.clone(),post.clone());let now_liked=if self.likes.remove(&key){let p=self.posts.get_mut(post).unwrap();p.likes=p.likes.saturating_sub(1);false}else{self.likes.insert(key);self.posts.get_mut(post).unwrap().likes+=1;true};if now_liked{self.notify(author,actor,NotificationKind::Like{post:post.clone()});}Ok(now_liked)}
    fn comment(&mut self,post:&PostId,text:String)->Result<Comment,SocialError>{validate_text("comment",&text,1,1000)?;let actor=self.actor()?;let author=self.posts.get(post).ok_or_else(||SocialError::NotFound("post".into()))?.author.clone();let comment=Comment{id:CommentId(self.next_id("comment")),post:post.clone(),author:actor.clone(),text,created_at:self.now()};self.comments.entry(post.clone()).or_default().push(comment.clone());self.posts.get_mut(post).unwrap().comments+=1;self.notify(author,actor,NotificationKind::Comment{post:post.clone()});Ok(comment)}
    fn comments(&self,post:&PostId)->Result<Vec<Comment>,SocialError>{if !self.posts.contains_key(post){return Err(SocialError::NotFound("post".into()));}Ok(self.comments.get(post).cloned().unwrap_or_default())}
    fn follow(&mut self,user:&UserId)->Result<bool,SocialError>{let actor=self.actor()?;if user==&actor{return Err(SocialError::InvalidInput("cannot follow yourself".into()));}if !self.users.contains_key(user){return Err(SocialError::NotFound("user".into()));}let key=(actor.clone(),user.clone());let following=if self.follows.remove(&key){if let Some(p)=self.users.get_mut(&actor){p.following=p.following.saturating_sub(1);}if let Some(p)=self.users.get_mut(user){p.followers=p.followers.saturating_sub(1);}false}else{self.follows.insert(key);self.users.get_mut(&actor).unwrap().following+=1;self.users.get_mut(user).unwrap().followers+=1;true};if following{self.notify(user.clone(),actor,NotificationKind::Follow{user:user.clone()});}Ok(following)}
    fn search_profiles(&self,query:&str,limit:usize)->Vec<Profile>{let q=query.trim().to_lowercase();self.users.values().filter(|p|q.is_empty()||p.handle.to_lowercase().contains(&q)||p.display_name.to_lowercase().contains(&q)).take(limit).cloned().collect()}
    fn notifications(&self)->Result<Vec<Notification>,SocialError>{Ok(self.notifications.iter().filter(|n|n.recipient==self.me).rev().cloned().collect())}
}

fn validate_text(field:&str,value:&str,min:usize,max:usize)->Result<(),SocialError>{let n=value.chars().count();if n<min||n>max{Err(SocialError::InvalidInput(format!("{field} must be {min}..={max} characters")))}else{Ok(())}}
fn ms(t:SystemTime)->u128{t.duration_since(UNIX_EPOCH).unwrap_or_default().as_millis()}

#[derive(Clone,Debug,PartialEq,Eq)] pub enum SocialError{Unauthorized,Forbidden,NotFound(String),InvalidInput(String),Transport(String)}
impl fmt::Display for SocialError{fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result{match self{Self::Unauthorized=>f.write_str("unauthorized"),Self::Forbidden=>f.write_str("forbidden"),Self::NotFound(x)=>write!(f,"not found: {x}"),Self::InvalidInput(x)=>write!(f,"invalid input: {x}"),Self::Transport(x)=>write!(f,"transport: {x}")}}}
impl std::error::Error for SocialError{}

pub fn demo_profile(id:&str,handle:&str,name:&str)->Profile{Profile{id:UserId(id.into()),handle:handle.into(),display_name:name.into(),bio:String::new(),followers:0,following:0,posts:0}}

#[cfg(test)] mod tests{
use super::*;
fn backend()->MemorySocialBackend{let mut b=MemorySocialBackend::new(demo_profile("u1","teja","Teja"));b.add_profile(demo_profile("u2","maya","Maya"));b}
fn upload(b:&mut MemorySocialBackend)->AssetId{b.upload_asset(UploadRequest{bytes:vec![1,2,3],duration_ns:9,title:"moment".into()}).unwrap().id}
#[test]fn publish_like_comment_and_feed(){let mut b=backend();let a=upload(&mut b);let p=b.publish(PublishRequest{asset:a,caption:"hello".into(),privacy:Privacy::Public,remix_of:None}).unwrap();assert_eq!(b.feed(FeedKind::ForYou,None,None,10).unwrap().posts.len(),1);assert!(b.like(&p.id).unwrap());b.comment(&p.id,"wow".into()).unwrap();let p2=b.feed(FeedKind::ForYou,None,None,10).unwrap().posts.remove(0);assert_eq!((p2.likes,p2.comments),(1,1));}
#[test]fn follower_privacy_is_enforced(){let mut b=backend();let a=upload(&mut b);b.publish(PublishRequest{asset:a,caption:"followers".into(),privacy:Privacy::Followers,remix_of:None}).unwrap();b.set_active_user(UserId("u2".into())).unwrap();assert!(b.feed(FeedKind::ForYou,None,None,10).unwrap().posts.is_empty());b.follow(&UserId("u1".into())).unwrap();assert_eq!(b.feed(FeedKind::ForYou,None,None,10).unwrap().posts.len(),1);}
#[test]fn asset_round_trip(){let mut b=backend();let id=upload(&mut b);assert_eq!(b.download_asset(&id).unwrap(),vec![1,2,3]);}
#[test]fn notifications_reach_other_user(){let mut b=backend();b.follow(&UserId("u2".into())).unwrap();b.set_active_user(UserId("u2".into())).unwrap();assert_eq!(b.notifications().unwrap().len(),1);}
}
