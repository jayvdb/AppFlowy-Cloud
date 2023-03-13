use crate::component::auth::Token;
use actix_session::SessionExt;
use actix_session::{Session, SessionGetError, SessionInsertError};
use actix_web::dev::Payload;
use actix_web::{FromRequest, HttpRequest};
use secrecy::{ExposeSecret, Secret};
use std::future::{ready, Ready};
use uuid::Uuid;

pub struct SessionToken(Session);

impl SessionToken {
    const TOKEN_ID_KEY: &'static str = "token";

    pub fn renew(&self) {
        self.0.renew();
    }

    pub fn insert_token(&self, token: Secret<Token>) -> Result<(), SessionInsertError> {
        self.0.insert(Self::TOKEN_ID_KEY, token.expose_secret())
    }

    pub fn get_token(&self) -> Result<Option<String>, SessionGetError> {
        self.0.get(Self::TOKEN_ID_KEY)
    }

    pub fn log_out(self) {
        self.0.purge()
    }
}

impl FromRequest for SessionToken {
    type Error = <Session as FromRequest>::Error;
    type Future = Ready<Result<SessionToken, Self::Error>>;

    fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        ready(Ok(SessionToken(req.get_session())))
    }
}