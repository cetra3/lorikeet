use crate::step::output_renderer;

use super::STEP_OUTPUT;
use regex::Regex;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use reqwest::{
    header::{HeaderValue, COOKIE, SET_COOKIE},
    multipart::Form,
    multipart::Part,
    redirect::Policy,
    Body, Method,
};

use tokio::fs::File;

use chashmap::CHashMap;
use lazy_static::lazy_static;

use cookie::{Cookie, CookieJar};

use tokio_util::codec::{BytesCodec, FramedRead};

use std::collections::HashMap;
use std::{path::PathBuf, str::FromStr};

lazy_static! {
    static ref COOKIES: CHashMap<String, CookieJar> = CHashMap::new();
    static ref REGEX_OUTPUT: Regex = Regex::new("\\$\\{(step_output.[^}]+)\\}").unwrap();
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum HttpVariant {
    UrlOnly(String),
    Options(Box<HttpOptions>),
}

fn method_to_string<S>(method: &Method, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    s.serialize_str(method.as_ref())
}

fn string_to_method<'de, D>(d: D) -> Result<Method, D::Error>
where
    D: Deserializer<'de>,
{
    Deserialize::deserialize(d)
        .and_then(|val: String| Method::from_str(&val).map_err(serde::de::Error::custom))
}

fn default_cookies() -> bool {
    true
}

fn default_status() -> u16 {
    200
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HttpOptions {
    url: String,
    #[serde(
        default,
        deserialize_with = "string_to_method",
        serialize_with = "method_to_string"
    )]
    method: Method,
    #[serde(default = "default_cookies")]
    save_cookies: bool,
    #[serde(default = "default_status")]
    status: u16,
    #[serde(default)]
    headers: Option<HashMap<String, String>>,
    #[serde(default)]
    user: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    pass: Option<String>,
    #[serde(default)]
    form: Option<HashMap<String, String>>,
    #[serde(default)]
    multipart: Option<HashMap<String, MultipartValue>>,
    #[serde(default)]
    verify_ssl: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MultipartValue {
    Value(String),
    Path(PathStruct),
    Step(StepStruct),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PathStruct {
    file: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StepStruct {
    step: String,
}

impl HttpVariant {
    pub async fn run(&self) -> Result<String, String> {
        let mut httpops = match *self {
            HttpVariant::UrlOnly(ref val) => HttpOptions {
                url: val.clone(),
                method: Method::GET,
                status: default_status(),
                headers: None,
                save_cookies: default_cookies(),
                user: None,
                pass: None,
                body: None,
                form: None,
                multipart: None,
                verify_ssl: None,
            },
            HttpVariant::Options(ref opts) => *opts.clone(),
        };

        let mut client_builder = reqwest::ClientBuilder::new().redirect(Policy::none());

        if let Some(verify_ssl) = httpops.verify_ssl {
            client_builder = client_builder.danger_accept_invalid_certs(!verify_ssl);
        }

        let client = client_builder.build().map_err(|err| format!("{}", err))?;

        let url = reqwest::Url::from_str(&httpops.url)
            .map_err(|err| format!("Failed to parse url `{}`: {}", httpops.url, err))?;

        let hostname: String = url
            .host_str()
            .map(String::from)
            .ok_or_else(|| format!("No host could be found for url: {}", url))?;

        if (httpops.form.is_some() || httpops.multipart.is_some() || httpops.body.is_some())
            && httpops.method == Method::GET
        {
            httpops.method = Method::POST;
        }

        let mut request = client.request(httpops.method, url);

        if let Some(user) = httpops.user {
            request = request.basic_auth(user, httpops.pass)
        }

        if let Some(form) = httpops.form {
            request = request.form(&form)
        }

        if let Some(multipart) = httpops.multipart {
            let mut form = Form::new();

            for (key, val) in multipart.into_iter() {
                form = match val {
                    MultipartValue::Value(string) => form.text(key, string),
                    MultipartValue::Path(path_struct) => {
                        let file_name = path_struct
                            .file
                            .file_name()
                            .map(|val| val.to_string_lossy().to_string())
                            .unwrap_or_default();

                        let file = File::open(&path_struct.file)
                            .await
                            .map_err(|err| format!("{:?}", err))?;
                        let reader = Body::wrap_stream(FramedRead::new(file, BytesCodec::new()));
                        form.part(key, Part::stream(reader).file_name(file_name))
                    }
                    MultipartValue::Step(step) => match STEP_OUTPUT.get(&step.step) {
                        Some(val) => form.text(key, val.to_string()),
                        None => return Err(format!("Step {} could not be found", &step.step)),
                    },
                }
            }

            request = request.multipart(form)
        }

        if let Some(body) = httpops.body {
            request = request.body(output_renderer(&body)?);
        }

        if let Some(cookie_jar) = COOKIES.get(&hostname) {
            let cookie_strings: Vec<String> = cookie_jar.iter().map(Cookie::to_string).collect();
            request = request.header(COOKIE, cookie_strings.join("; "))
        }

        if let Some(headers) = httpops.headers {
            for (key, val) in headers.into_iter() {
                request = request.header(&*key, &*val);
            }
        }

        let response = client
            .execute(request.build().map_err(|err| format!("{:?}", err))?)
            .await
            .map_err(|err| format!("Error connecting to url {}", err))?;

        if response.status().as_u16() != httpops.status {
            return Err(format!(
                "returned status `{}` does not match expected `{}`",
                response.status().as_u16(),
                httpops.status
            ));
        }

        if httpops.save_cookies {
            let new_cookies = response.headers().get_all(SET_COOKIE);

            COOKIES.alter(hostname, |value| {
                let mut cookie_jar = value.unwrap_or_default();
                for cookie in new_cookies
                    .iter()
                    .flat_map(HeaderValue::to_str)
                    .map(String::from)
                    .flat_map(Cookie::parse)
                {
                    cookie_jar.add(cookie);
                }
                Some(cookie_jar)
            });
        }

        let output = response.text().await.map_err(|err| format!("{:?}", err))?;

        Ok(output)
    }
}
