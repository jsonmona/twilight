use std::{
    collections::{BTreeMap, HashMap},
    fmt::{Debug, Display},
    ops::Deref,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Weak,
    },
    time::{Duration, Instant},
};

use actix_web::{FromRequest, HttpResponse, ResponseError};
use anyhow::{anyhow, Result};
use parking_lot::Mutex;
use rand::thread_rng;

use crate::server::session_id::SessionId;

const EXPIRE_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// The actual type that's passed around
pub type Sessions = Mutex<SessionStorage>;

pub struct SessionStorage {
    sessions: HashMap<SessionId, Weak<Session>>,
    last_used: BTreeMap<(Instant, SessionId), Arc<Session>>,
}

pub struct Session {
    sid: SessionId,
    stream_count: AtomicUsize,
    last_used: Mutex<Instant>,
}

impl SessionStorage {
    pub fn new() -> Sessions {
        Mutex::new(Self {
            sessions: Default::default(),
            last_used: Default::default(),
        })
    }

    pub fn create_session(&mut self) -> Result<Arc<Session>> {
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
        let session = Arc::new(Session {
            sid: sid.clone(),
            stream_count: AtomicUsize::new(0),
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

    pub fn access(&mut self, sid: &SessionId) -> Option<Arc<Session>> {
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
            //FIXME: is relaxed ordering enough?
            if entry.get().stream_count.load(Ordering::Relaxed) == 0 {
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

impl Session {
    pub fn sid(&self) -> &SessionId {
        &self.sid
    }

    pub fn open_stream(self: &Arc<Self>) -> Result<StreamGuard> {
        let prev_cnt = self.stream_count.fetch_add(1, Ordering::Relaxed);
        if (isize::MAX - 1) as usize <= prev_cnt {
            Err(anyhow!("too many stream open for {:?}", self.sid))
        } else {
            Ok(StreamGuard(Arc::clone(self)))
        }
    }
}

pub struct StreamGuard(Arc<Session>);

impl Drop for StreamGuard {
    fn drop(&mut self) {
        self.0.stream_count.fetch_sub(1, Ordering::Relaxed);
    }
}

pub struct SessionGuard(pub Arc<Session>);

impl FromRequest for SessionGuard {
    type Error = UnauthorizedError;

    type Future = std::future::Ready<Result<Self, Self::Error>>;

    fn from_request(
        req: &actix_web::HttpRequest,
        _payload: &mut actix_web::dev::Payload,
    ) -> Self::Future {
        let f = move || {
            let storage = req.app_data::<Sessions>()?;

            let auth = req.headers().get(actix_web::http::header::AUTHORIZATION)?;
            let auth = std::str::from_utf8(auth.as_bytes())
                .ok()?
                .to_ascii_lowercase();

            let token = auth.strip_prefix("bearer")?;
            let sid = SessionId::from_hex(token)?;

            Some(Self(storage.lock().access(&sid)?))
        };

        std::future::ready(f().ok_or(UnauthorizedError))
    }
}

impl Deref for SessionGuard {
    type Target = Arc<Session>;

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
