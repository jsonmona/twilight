use std::{
    collections::{BTreeMap, HashMap},
    fmt::{Debug, Display},
    ops::Deref,
    sync::{Arc, Weak},
    time::{Duration, Instant},
};

use actix::{Addr, WeakAddr};
use actix_web::{web, FromRequest, HttpResponse, ResponseError};
use anyhow::{anyhow, Result};
use parking_lot::{Mutex, RwLock};
use rand::thread_rng;
use rustc_hash::FxHashMap;

use crate::server::{Channel, TwilightServer};

use super::{SessionId, WebsocketActor};

const EXPIRE_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// The actual type that's passed around
pub type Sessions = Mutex<SessionStorage>;

pub struct SessionStorage {
    sessions: HashMap<SessionId, Weak<WebSession>>,
    last_used: BTreeMap<(Instant, SessionId), Arc<WebSession>>,
}

pub struct WebSession {
    sid: SessionId,
    channels: RwLock<FxHashMap<u16, Arc<Channel>>>,
    stream: RwLock<Option<WeakAddr<WebsocketActor>>>,
    last_used: Mutex<Instant>,
}

pub struct SessionGuard(pub Arc<WebSession>);

impl SessionStorage {
    pub fn new() -> Sessions {
        Mutex::new(Self {
            sessions: Default::default(),
            last_used: Default::default(),
        })
    }

    pub fn create_session(&mut self) -> Result<Arc<WebSession>> {
        self.expire();

        let mut rng = thread_rng();
        let mut sid = None;

        for _ in 0..1000 {
            let id = SessionId::from_random(&mut rng);
            if !self.sessions.contains_key(&id) {
                // Found empty slot
                sid = Some(id);
                break;
            }
        }

        let sid = sid.ok_or_else(|| anyhow!("unable to find empty session slot"))?;
        let now = Instant::now();
        let session = Arc::new(WebSession {
            sid: sid.clone(),
            channels: Default::default(),
            stream: RwLock::new(None),
            last_used: Mutex::new(now),
        });

        // put into sessions
        if self
            .sessions
            .insert(sid.clone(), Arc::downgrade(&session))
            .is_some()
        {
            panic!("it must be non existing session id");
        }

        // put into last_used
        self.last_used
            .insert((now, sid.clone()), Arc::clone(&session));

        Ok(session)
    }

    pub fn access(&mut self, sid: &SessionId) -> Option<Arc<WebSession>> {
        self.expire();

        let session = self.sessions.get(sid)?.upgrade()?;

        // Update last_used table
        {
            // no deadlock because this is the only code locking last_used
            let mut last_used = session.last_used.lock();
            self.last_used.remove(&(*last_used, sid.clone()));

            let now = Instant::now();
            *last_used = now;
            self.last_used
                .insert((now, sid.clone()), Arc::clone(&session));
        }

        Some(session)
    }

    fn expire(&mut self) {
        let valid_after = Instant::now() - EXPIRE_TIMEOUT;

        while let Some(entry) = self.last_used.first_entry() {
            if &valid_after <= &entry.key().0 {
                // minimum instant in this table is valid. No need to check more.
                break;
            }

            // last_used is before valid_after. Expire session if no stream is open.
            if entry.get().is_stream_open() {
                self.sessions.remove(&entry.key().1);
                entry.remove();
            }
        }
    }
}

impl Debug for SessionStorage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionStorage")
            .field("session_count", &self.sessions.len())
            .field(
                "last_used",
                &self
                    .last_used
                    .iter()
                    .map(|x| x.0.clone())
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl WebSession {
    pub fn sid(&self) -> &SessionId {
        &self.sid
    }

    pub fn stream(&self) -> Option<Addr<WebsocketActor>> {
        self.stream.read().as_ref().and_then(|x| x.upgrade())
    }

    pub fn is_stream_open(&self) -> bool {
        self.stream.read().is_some()
    }

    pub fn open_stream(&self, addr: WeakAddr<WebsocketActor>) -> Result<()> {
        let mut stream = self.stream.write();
        if stream.is_some() {
            return Err(anyhow!(
                "tried to open stream when already open for {:?}",
                self.sid
            ));
        }

        *stream = Some(addr);
        Ok(())
    }

    pub fn close_stream(&self) -> Result<()> {
        let mut stream = self.stream.write();
        if stream.is_none() {
            return Err(anyhow!(
                "tried to close stream when not open for {:?}",
                self.sid
            ));
        }

        *stream = None;
        Ok(())
    }

    pub fn create_channel(&self, server: &mut TwilightServer) -> Arc<Channel> {
        let channel = server.create_channel();
        self.channels
            .write()
            .insert(channel.ch, Arc::clone(&channel));
        channel
    }

    pub fn get_channel(&self, ch: u16) -> Option<Arc<Channel>> {
        self.channels.read().get(&ch).map(|x| Arc::clone(x))
    }

    pub fn close_channel(&self, ch: u16) {
        self.channels.write().remove(&ch);
    }
}

impl FromRequest for SessionGuard {
    type Error = UnauthorizedError;

    type Future = std::future::Ready<Result<Self, Self::Error>>;

    fn from_request(
        req: &actix_web::HttpRequest,
        _payload: &mut actix_web::dev::Payload,
    ) -> Self::Future {
        let f = move || {
            let storage = req.app_data::<web::Data<Sessions>>()?;

            let auth = req.headers().get(actix_web::http::header::AUTHORIZATION)?;
            let auth = std::str::from_utf8(auth.as_bytes()).ok()?;

            let token = auth.strip_prefix("Bearer ")?.trim();
            let sid = SessionId::from_hex(token)?;

            Some(Self(storage.lock().access(&sid)?))
        };

        std::future::ready(f().ok_or(UnauthorizedError))
    }
}

impl Deref for SessionGuard {
    type Target = Arc<WebSession>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug)]
pub struct UnauthorizedError;

impl Display for UnauthorizedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("UnauthorizedError")
    }
}

impl ResponseError for UnauthorizedError {
    fn status_code(&self) -> actix_web::http::StatusCode {
        actix_web::http::StatusCode::FORBIDDEN
    }

    fn error_response(&self) -> actix_web::HttpResponse<actix_web::body::BoxBody> {
        HttpResponse::build(self.status_code()).finish()
    }
}
