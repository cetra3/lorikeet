use std::process::Command;
use regex::Regex;

use reqwest::Method;
use std::time::{Duration, Instant};
use serde::de::{Error, Deserialize, Deserializer};
use serde::ser::Serializer;

use jmespath;


use std::str::FromStr;
use std::io::Read;

use std::collections::HashMap;

use hyper::header::{SetCookie, Cookie};

use sys_info::{loadavg, mem_info, disk_info};

use chashmap::CHashMap;

use reqwest::{self, RedirectPolicy};


#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Outcome {
    pub output: Option<String>,
    pub error: Option<String>,
    pub duration: Duration
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Step {
    pub name: String,
    pub description: Option<String>,
    pub run: RunType,
    pub filters: Vec<FilterType>,
    pub expect: ExpectType,
    pub outcome: Option<Outcome>,
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunType {
    Value(String),
    Bash(BashVariant),
    Http(HttpVariant),
    System(SystemVariant)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemVariant {
    MemTotal,
    MemFree,
    MemAvailable,
    LoadAvg1m,
    LoadAvg5m,
    LoadAvg15m,
    DiskTotal,
    DiskFree
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BashVariant {
    CmdOnly(String),
    Options(BashOptions)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum HttpVariant {
    UrlOnly(String),
    Options(HttpOptions)
}

lazy_static! {
    static ref COOKIES: CHashMap<String, Cookie> = CHashMap::new();
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HttpOptions {
    url: String,
    #[serde(default, deserialize_with = "string_to_method", serialize_with = "method_to_string")]
    method: Method,
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BashOptions {
    cmd: String
}

fn method_to_string<S>(method: &Method, s: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
    s.serialize_str(method.as_ref())
}

fn string_to_method<'de,D>(d: D) -> Result<Method, D::Error>
    where D: Deserializer<'de> {
    Deserialize::deserialize(d).and_then(|str| Method::from_str(str).map_err(Error::custom))
}

fn default_cookies() -> bool { false }

fn default_status() -> u16 {
    200
}

impl RunType {
    pub fn execute(&self, expect: ExpectType, filters: Vec<FilterType>) -> Outcome {

        let start = Instant::now();


        return self.run()
            .and_then(|mut val| {

                for filter in filters {
                    val = filter.filter(&val)?;
                };

                Ok(val)
            })
            .map(|val| {

            let expect = expect.check(&val);

            let error = match expect {
                Ok(_) => None,
                Err(err) => Some(err)
            };

            return Outcome {
                output: val.into(),
                error: error,
                duration: start.elapsed()
            }

        }).unwrap_or_else(|val| {
            return Outcome {
                output: None,
                error: val.into(),
                duration: start.elapsed()
            }
        });
  
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
                        }
                    },
                    BashVariant::Options(ref opts) => {
                        opts.clone()
                    }
                };

                match Command::new("bash").arg("-c").arg(bashopts.cmd).output() {
                    Ok(output) => {
                        if output.status.success() {
                            Ok(format!("{}", String::from_utf8_lossy(&output.stdout)))
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

                let mut clientbuilder = reqwest::ClientBuilder::new();

                let client = clientbuilder.redirect(RedirectPolicy::none()).build().map_err(|err| format!("{}", err))?;

                let url = reqwest::Url::from_str(&httpops.url).map_err(|err| format!("Failed to parse url `{}`: {}", httpops.url, err))?;

                let hostname: String = url.host_str().map(|str| String::from(str)).ok_or_else(|| format!("No host could be found for url: {}", url))?;

                if httpops.form != None && httpops.method == Method::Get {
                    httpops.method = Method::Post;
                }

                let mut request = client.request(httpops.method, url);

                if httpops.user != None {
                    request.basic_auth(httpops.user.unwrap(), httpops.pass);
                }

                if let Some(form) = httpops.form {
                    request.form(&form);
                }


                if let Some(cookies) = COOKIES.get(&hostname) {
                    request.header(cookies.clone());
                }




                let mut response = client.execute(request.build().map_err(|err| format!("{:?}", err))?).map_err(|err| {
                    format!("Error connecting to url {}", err)
                })?;
                let mut output = String::new();

                if response.status().as_u16() != httpops.status {
                    return Err(format!("returned status `{}` does not match expected `{}`", response.status().as_u16(), httpops.status));
                }

                response.read_to_string(&mut output).map_err(|err| format!("{:?}", err))?;

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

            },
            RunType::System(ref variant) => {
                match *variant {
                    SystemVariant::LoadAvg1m => {
                        loadavg().map(|load| load.one.to_string()).map_err(|_| String::from(format!("Could not get load")))
                    },
                    SystemVariant::LoadAvg5m => {
                        loadavg().map(|load| load.five.to_string()).map_err(|_| String::from(format!("Could not get load")))
                    },
                    SystemVariant::LoadAvg15m => {
                        loadavg().map(|load| load.fifteen.to_string()).map_err(|_| String::from(format!("Could not get load")))
                    },
                    SystemVariant::MemAvailable => {
                        mem_info().map(|mem| mem.avail.to_string()).map_err(|_| String::from(format!("Could not get memory")))
                    },
                    SystemVariant::MemFree => {
                        mem_info().map(|mem| mem.free.to_string()).map_err(|_| String::from(format!("Could not get memory")))
                    },
                    SystemVariant::MemTotal => {
                        mem_info().map(|mem| mem.total.to_string()).map_err(|_| String::from(format!("Could not get memory")))
                    },
                    SystemVariant::DiskTotal => {
                        disk_info().map(|disk| disk.total.to_string()).map_err(|_| String::from(format!("Could not get disk")))
                    }
                    SystemVariant::DiskFree => {
                        disk_info().map(|disk| disk.free.to_string()).map_err(|_| String::from(format!("Could not get disk")))
                    }
                }
            }
        }
    }
}


impl Step {
    pub fn get_duration_ms(&self) -> f32 {

        match self.outcome {
            Some(ref outcome) => {
                let nanos = outcome.duration.subsec_nanos() as f32;
                (1000000000f32 * outcome.duration.as_secs() as f32 + nanos)/(1000000f32)
            },
            None => 0f32
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FilterType {
    NoOutput,
    Regex(RegexVariant),
    JmesPath(String)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RegexVariant {
    MatchOnly(String),
    Options(RegexOptions)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RegexOptions {
    matches: String,
    group: String
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExpectType {
    Anything,
    Matches(String),
    GreaterThan(f64),
    LessThan(f64)
}

impl FilterType {
    fn filter(&self, val: &str) -> Result<String, String> {
        match *self {
            FilterType::NoOutput => Ok(String::from("")),
            FilterType::JmesPath(ref jmes) => {

                let expr = jmespath::compile(jmes).map_err(|err| format!("Could not compile jmespath:{}", err))?;

                let data = jmespath::Variable::from_json(val).map_err(|err| format!("Could not format as json:{}", err))?;

                let result = expr.search(data).map_err(|err| format!("Could not find jmes expression:{}", err))?;

                let output = (*result).to_string();

                if output != "null" {
                    Ok(output)
                } else {
                    Err(format!("Could not find jmespath expression `{}` in output", expr))
                }
                
            },
            FilterType::Regex(ref regex_var) => {

                let opts = match regex_var {
                    &RegexVariant::MatchOnly(ref string) => {
                        RegexOptions {
                            matches: string.clone(),
                            group: "0".into()
                        }
                    },
                    &RegexVariant::Options(ref opts) => opts.clone()
                };

                let regex = Regex::new(&opts.matches).map_err(|err| format!("Could not create regex from `{}`.  Error is:{:?}", &opts.matches, err))?;

                let captures = regex.captures(val).ok_or_else(|| format!("Could not find `{}` in output", &opts.matches))?;

                match opts.group.parse::<usize>() {
                    Ok(num) => {
                        return captures.get(num).map(|val| val.as_str().into()).ok_or_else(|| format!("Could not find group number `{}` in regex `{}`", opts.group, opts.matches));
                    },
                    Err(_) => {
                        return captures.name(&opts.group).map(|val| val.as_str().into()).ok_or_else(|| format!("Could not find group name `{}` in regex `{}`", opts.group, opts.matches));
                    }
                }


            }
        }
    }
}

impl ExpectType {
    fn check(&self, val: &str) -> Result<(), String> {

        let number_filter = Regex::new("[^0-9.,]").unwrap();

        match *self {
            ExpectType::Anything => Ok(()),
            ExpectType::Matches(ref match_string) => {
                let regex = Regex::new(match_string).map_err(|err| format!("Could not create regex from `{}`.  Error is:{:?}", match_string, err))?;

                if regex.is_match(val) {
                    Ok(())
                } else {
                    Err(format!("Not matched against `{}`", match_string))
                }
            },
            ExpectType::GreaterThan(ref num) => {

                match number_filter.replace_all(val, "").parse::<f64>() {
                    Ok(compare) => {
                        if compare > *num {
                            Ok(())
                        } else {
                            Err(format!("The value `{}` is not greater than `{}`", compare, num))
                        }
                    },
                    Err(_) => {
                        Err(format!("Could not parse `{}` as a number", val))
                    }
                }
            },
            ExpectType::LessThan(ref num) => {
                match number_filter.replace_all(val, "").parse::<f64>() {
                    Ok(compare) => {
                        if compare < *num {
                            Ok(())
                        } else {
                            Err(format!("The value `{}` not less than `{}`", compare, num))
                        }
                    },
                    Err(_) => {
                        Err(format!("Could not parse `{}` as a number", num))
                    }
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

