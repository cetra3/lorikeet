use std::process::Command;
use regex::Regex;

use reqwest::Method;
use std::time::{Duration, Instant};
use serde::de::{Error, Deserialize, Deserializer};

use std::str::FromStr;
use std::io::Read;

use std::collections::HashMap;

use hyper::header::{SetCookie, Cookie};

use term_painter::Color::*;
use term_painter::ToStyle;

use chashmap::CHashMap;

use reqwest::{self, RedirectPolicy};


#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Outcome {
    pub result: Result<String, String>,
    pub duration: Duration
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Step {
    pub name: String,
    pub run: RunType,
    pub expect: ExpectType,
    pub require: Vec<String>,
    pub required_by: Vec<String>
}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum Requirement {
    Some(String),
    Many(Vec<String>)
}

impl Requirement {
    pub fn to_vec(&self) -> Vec<String> {
        match *self {
            Requirement::Some(ref string) => vec![string.clone()],
            Requirement::Many(ref vec) => vec.clone()
        }
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunType {
    Value(String),
    Bash(BashVariant),
    Http(HttpVariant)
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum BashVariant {
    CmdOnly(String),
    Options(BashOptions)
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum HttpVariant {
    UrlOnly(String),
    Options(HttpOptions)
}

lazy_static! {
    static ref COOKIES: CHashMap<String, Cookie> = CHashMap::new();
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct HttpOptions {
    url: String,
    #[serde(default, deserialize_with = "string_to_method")]
    method: Method,
    #[serde(default = "default_output")]
    get_output: bool,
    #[serde(default = "default_cookies")]
    save_cookies: bool,
    #[serde(default = "default_status")]
    status: u16,
    #[serde(default)]
    user: Option<String>,
    #[serde(default)]
    pass: Option<String>,
    #[serde(default)]
    form: Option<HashMap<String, String>>
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct BashOptions {
    cmd: String,
    #[serde(default = "default_output")]
    get_output: bool
}

fn string_to_method<'de,D>(d: D) -> Result<Method, D::Error>
    where D: Deserializer<'de> {
    Deserialize::deserialize(d).and_then(|str| Method::from_str(str).map_err(Error::custom))
}

fn default_cookies() -> bool { false }

fn default_output() -> bool {
    true
}

fn default_status() -> u16 {
    200
}

impl RunType {
    pub fn execute(&self, expect: ExpectType) -> Outcome {

        let start = Instant::now();

        let result = self.run().and_then(|val| expect.check(&val));

        Outcome {
            result: result,
            duration: start.elapsed()
        }
    }

    fn run(&self) -> Result<String, String> {
        match *self {
            RunType::Value(ref val) => {
                Ok(val.clone())
            },
            RunType::Bash(ref val) => {

                let bashopts = match *val {
                    BashVariant::CmdOnly(ref val) => {
                        BashOptions {
                            cmd: val.clone(),
                            get_output: default_output()
                        }
                    },
                    BashVariant::Options(ref opts) => {
                        opts.clone()
                    }
                };

                match Command::new("bash").arg("-c").arg(bashopts.cmd).output() {
                    Ok(output) => {
                        if output.status.success() {
                            if bashopts.get_output {
                                Ok(format!("{}", String::from_utf8_lossy(&output.stdout)))
                            } else {
                                Ok(String::new())
                            }
                        } else {
                            Err(format!("Exit Code:{}, StdErr:{}, StdOut:{}", output.status.code().unwrap_or(1), String::from_utf8_lossy(&output.stderr), String::from_utf8_lossy(&output.stdout)))
                        }
                    },
                    Err(err) => {
                        Err(format!("Err:{:?}", err))
                    }
                }
            },
            RunType::Http(ref val) => {

                let mut httpops = match *val {
                    HttpVariant::UrlOnly(ref val) => {
                        HttpOptions {
                            url: val.clone(),
                            method: Method::Get,
                            get_output: default_output(),
                            status: default_status(),
                            save_cookies: default_cookies(),
                            user: None,
                            pass: None,
                            form: None
                        }
                    },
                    HttpVariant::Options(ref opts) => {
                        opts.clone()
                    }
                };

                let mut clientbuilder = reqwest::ClientBuilder::new().map_err(|err| format!("{:?}", err))?;

                let client = clientbuilder.redirect(RedirectPolicy::none()).build().map_err(|err| format!("{:?}", err))?;

                let url = reqwest::Url::from_str(&httpops.url).map_err(|err| format!("{:?}", err))?;

                let hostname: String = url.host_str().map(|str| String::from(str)).ok_or_else(|| format!("No host could be found for url:{}", url))?;

                if httpops.form != None && httpops.method == Method::Get {
                    httpops.method = Method::Post;
                }

                let mut request = client.request(httpops.method, url).map_err(|err| format!("{:?}", err))?;

                if httpops.user != None {
                    request.basic_auth(httpops.user.unwrap(), httpops.pass);
                }

                if let Some(form) = httpops.form {
                    request.form(&form).expect("Could not serialize form!");
                }


                if let Some(cookies) = COOKIES.get(&hostname) {
                    request.header(cookies.clone());
                }




                let mut response = client.execute(request.build()).map_err(|err| format!("{:?}", err))?;
                let mut output = String::new();

                if response.status().as_u16() != httpops.status {
                    return Err(format!("returned status `{}` does not match expected `{}`", response.status().as_u16(), httpops.status));
                }

                if httpops.get_output {
                    response.read_to_string(&mut output).map_err(|err| format!("{:?}", err))?;
                }

                if httpops.save_cookies {

                    if let Some(cookies) = response.headers().get::<SetCookie>() {

                            let mut new_cookies = Cookie::new();

                            for cookie in cookies.iter() {

                                let cookie_parts: Vec<&str> = cookie.split(";").collect();
                                let key_value: Vec<&str> = cookie_parts[0].splitn(2, "=", ).collect();

                                new_cookies.set(String::from(key_value[0]), String::from(key_value[1]));

                            }

                            COOKIES.insert(hostname, new_cookies);
                    }

                }

                return Ok(output)

            }
        }
    }
}


impl Outcome {
    pub fn terminal_print(&self, step: &Step, colours: &bool) {

        let (style, pass, msg) = match self.result {
            Ok(ref msg) => {
                (Green.bold(), true, msg)
            },
            Err(ref msg) => {
                (Red.bold(), false, msg)
            }
        };

        let mut message = format!("- name: {}\n  pass: {}\n", step.name, pass);

        if msg != "" {

            let output = msg.trim();

            if output.contains("\n") {
                message.push_str(&format!("  output: |\n    {}\n", output.replace("\n", "\n    ")));
            } else {
                message.push_str(&format!("  output: {}\n", output));
            }

        }

        let nanos = self.duration.subsec_nanos() as f32;
        let ms : f32 = (1000000000f32 * self.duration.as_secs() as f32 + nanos)/(1000000f32);
        message.push_str(&format!("  duration: {}ms\n", ms));

        if *colours {
            println!("{}", style.paint(message));
        } else {
            println!("{}", message);
        }

    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExpectType {
    Anything,
    Matches(String)
}

impl ExpectType {
    fn check(&self, val: &str) -> Result<String, String> {

        match *self {
            ExpectType::Anything => Ok(String::from(val)),
            ExpectType::Matches(ref match_string) => {
                let regex = Regex::new(match_string).map_err(|err| format!("Could not create regex from `{}`.  Error is:{:?}", match_string, err))?;

                if regex.is_match(val) {
                    Ok(String::new())
                } else {
                    Err(format!("Not matched against `{}`", match_string))
                }
            }
        }


    }
}

impl Default for ExpectType {
    fn default() -> Self {
        ExpectType::Anything
    }
}

