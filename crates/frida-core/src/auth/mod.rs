//! Authentication (PLAN.md §4.5, §8): Basic, Bearer, API key and OAuth2, combinable, inherited
//! from folders and the collection.

mod jwt;
mod loopback;
mod oauth2;
mod pkce;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

pub use jwt::decode_claims;
pub use loopback::{Callback, DEFAULT_REDIRECT_URI, Loopback};
pub use oauth2::{
    BrowserOpener, OAuth2Error, OAuth2Manager, REFRESH_SKEW, Token, TokenCall, TokenInfo,
};
pub use pkce::{code_challenge, code_verifier};

use crate::http::PreparedRequest;
use crate::model::{ApiKeyLocation, Auth, AuthScheme, Collection, Folder, Request};

/// The schemes that apply to a request: its own, or the nearest folder's, or the collection's.
/// `inherit` walks up; if nothing defines auth, the result is empty ("none").
///
/// `folders` are outermost first, as returned by `storage::load_folder_chain`.
pub fn effective_auth(
    request: &Request,
    folders: &[(String, Folder)],
    collection: &Collection,
) -> Vec<AuthScheme> {
    let chain = std::iter::once(&request.auth)
        .chain(folders.iter().rev().map(|(_, folder)| &folder.auth))
        .chain(std::iter::once(&collection.auth));
    for auth in chain {
        match auth {
            Auth::Inherit => continue,
            Auth::None => return Vec::new(),
            Auth::Custom { schemes } => return schemes.clone(),
        }
    }
    Vec::new()
}

/// Adds the auth headers / query parameters of `schemes` to `prepared`, in order. Auth
/// replaces a header of the same name set elsewhere (e.g. an old `Authorization` header).
/// OAuth2 tokens come from `oauth` (cached, refreshed or fetched) with `call`, whose client
/// carries the collection's TLS settings for the token endpoint.
///
/// Returns the sensitive values it added (tokens, keys, credentials) for masking.
pub async fn apply_auth(
    prepared: &mut PreparedRequest,
    schemes: &[AuthScheme],
    oauth: &OAuth2Manager,
    call: &TokenCall<'_>,
) -> Result<Vec<String>, OAuth2Error> {
    // Values that must be masked in history and logs.
    let mut sensitive = Vec::new();
    for scheme in schemes {
        match scheme {
            AuthScheme::Basic { username, password } => {
                let encoded = STANDARD.encode(format!("{username}:{password}"));
                sensitive.push(encoded.clone());
                sensitive.push(password.clone());
                set_header(prepared, "Authorization", format!("Basic {encoded}"));
            }
            AuthScheme::Bearer { token } => {
                sensitive.push(token.clone());
                set_header(prepared, "Authorization", format!("Bearer {token}"));
            }
            AuthScheme::ApiKey {
                location,
                name,
                value,
            } => {
                sensitive.push(value.clone());
                match location {
                    ApiKeyLocation::Header => set_header(prepared, name, value.clone()),
                    ApiKeyLocation::Query => add_query(prepared, name, value),
                }
            }
            AuthScheme::OAuth2(config) => {
                let token = oauth.token(config, call).await?;
                sensitive.push(token.access_token.clone());
                let placement = &config.token_placement;
                match placement.location {
                    ApiKeyLocation::Header => {
                        let value = if placement.prefix.trim().is_empty() {
                            token.access_token
                        } else {
                            format!("{} {}", placement.prefix.trim(), token.access_token)
                        };
                        set_header(prepared, &placement.name, value);
                    }
                    ApiKeyLocation::Query => {
                        add_query(prepared, &placement.name, &token.access_token)
                    }
                }
            }
        }
    }
    sensitive.retain(|s| !s.is_empty());
    Ok(sensitive)
}

fn set_header(prepared: &mut PreparedRequest, name: &str, value: String) {
    prepared
        .headers
        .retain(|(n, _)| !n.eq_ignore_ascii_case(name));
    prepared.headers.push((name.to_string(), value));
}

fn add_query(prepared: &mut PreparedRequest, name: &str, value: &str) {
    // An unparsable URL is left alone; sending reports it as an invalid URL anyway.
    if let Ok(mut url) = reqwest::Url::parse(&prepared.url) {
        url.query_pairs_mut().append_pair(name, value);
        prepared.url = url.to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Folder;
    use tokio_util::sync::CancellationToken;

    fn custom(schemes: Vec<AuthScheme>) -> Auth {
        Auth::Custom { schemes }
    }
    fn bearer(token: &str) -> AuthScheme {
        AuthScheme::Bearer {
            token: token.into(),
        }
    }

    #[test]
    fn inheritance_walks_up_to_the_nearest_definition() {
        let mut collection = Collection::new("c");
        collection.auth = custom(vec![bearer("collection")]);
        let mut outer = Folder::new("outer");
        outer.auth = custom(vec![bearer("outer")]);
        let inner = Folder::new("inner"); // inherits
        let folders = vec![
            ("outer".to_string(), outer),
            ("outer/inner".to_string(), inner),
        ];
        let request = Request::new("r"); // inherits

        assert_eq!(
            effective_auth(&request, &folders, &collection),
            [bearer("outer")]
        );
        assert_eq!(
            effective_auth(&request, &[], &collection),
            [bearer("collection")]
        );

        let mut no_auth = Request::new("r");
        no_auth.auth = Auth::None;
        assert!(effective_auth(&no_auth, &folders, &collection).is_empty());

        let mut plain = Collection::new("c");
        plain.auth = Auth::Inherit; // nothing defines auth
        assert!(effective_auth(&request, &[], &plain).is_empty());
    }

    fn prepared(url: &str) -> PreparedRequest {
        PreparedRequest {
            method: "GET".into(),
            url: url.into(),
            headers: vec![("authorization".into(), "old".into())],
            body: crate::http::PreparedBody::None,
            timeout: std::time::Duration::from_secs(1),
            tls_verify: true,
            ca_cert_paths: vec![],
            proxy: Default::default(),
        }
    }

    #[tokio::test]
    async fn basic_bearer_and_api_keys_are_applied_in_order() {
        let oauth = OAuth2Manager::new(std::sync::Arc::new(|_| Ok(())));
        let client = reqwest::Client::new();
        let mut req = prepared("http://x/a?b=1");
        let schemes = vec![
            AuthScheme::Basic {
                username: "alice".into(),
                password: "pw".into(),
            },
            AuthScheme::ApiKey {
                location: ApiKeyLocation::Header,
                name: "x-api-key".into(),
                value: "k1".into(),
            },
            AuthScheme::ApiKey {
                location: ApiKeyLocation::Query,
                name: "api key".into(),
                value: "a&b".into(),
            },
        ];
        apply_auth(
            &mut req,
            &schemes,
            &oauth,
            &TokenCall {
                client: &client,
                cancel: &CancellationToken::new(),
                on_exchange: &|_| {},
                environment: None,
            },
        )
        .await
        .unwrap();

        assert!(
            req.headers
                .contains(&("Authorization".into(), "Basic YWxpY2U6cHc=".into()))
        );
        assert!(!req.headers.iter().any(|(_, v)| v == "old"));
        assert!(req.headers.contains(&("x-api-key".into(), "k1".into())));
        assert_eq!(req.url, "http://x/a?b=1&api+key=a%26b");

        let mut req = prepared("http://x/");
        apply_auth(
            &mut req,
            &[bearer("t")],
            &oauth,
            &TokenCall {
                client: &client,
                cancel: &CancellationToken::new(),
                on_exchange: &|_| {},
                environment: None,
            },
        )
        .await
        .unwrap();
        assert!(
            req.headers
                .contains(&("Authorization".into(), "Bearer t".into()))
        );
    }
}
