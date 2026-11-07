//! リクエスト単位のロケール解決と Cookie 永続化。

use axum::{
    extract::Request,
    http::header::SET_COOKIE,
    middleware::Next,
    response::Response,
};

use super::negotiate::{lang_cookie_value, resolve};
use super::CURRENT_LOCALE;

pub async fn set_request_locale(request: Request, next: Next) -> Response {
    let (locale, override_cookie) = resolve(request.headers(), request.uri());
    let set_cookie = override_cookie.map(Into::into);

    let mut response = CURRENT_LOCALE
        .scope(locale, async move { next.run(request).await })
        .await;

    if let Some(locale) = set_cookie {
        response
            .headers_mut()
            .append(SET_COOKIE, lang_cookie_value(locale));
    }

    response
}
