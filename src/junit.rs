use hostname;

use std::path::Path;
use std::fs::File;
use quick_xml::Writer;
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};

use submitter::StepResult;

use failure::Error;
use std::fs::create_dir_all;

pub fn create_junit(results: &Vec<StepResult>, file_path: &Path, hostname: Option<&str>) -> Result<(), Error> {


    if let Some(parent) = file_path.parent() {
        create_dir_all(parent)?;
    }

    let file = File::create(file_path)?;

    let mut writer = Writer::new_with_indent(file, b' ', 4);

    writer.write_event(Event::Decl(BytesDecl::new(b"1.0", Some(b"UTF-8"), None)))?;

    // Add in the testsuite elem

    let test_num = results.len();
    let skip_num = results.iter().filter(|step| {
        if let Some(ref output) = step.error {
            return output == "Dependency Not Met";
        }
        return false;
    }).count();
    let failure_num = results.iter().filter(|step| step.pass == false).count() - skip_num;

    let time = results.iter().fold(0f32, |sum, step| sum + step.duration);

    let hostname = match hostname {
        Some(hostname) => String::from(hostname),
        None => hostname::get_hostname().unwrap_or_else(||String::from(""))
    };

    let mut testsuite = BytesStart::borrowed(b"testsuite", b"testsuite".len());

    testsuite.push_attribute(("name", "lorikeet"));
    testsuite.push_attribute(("hostname", &*hostname));

    testsuite.push_attribute(("tests", &*test_num.to_string() ));
    testsuite.push_attribute(("failures", &*failure_num.to_string() ));
    testsuite.push_attribute(("skipped", &*skip_num.to_string() ));
    testsuite.push_attribute(("time", &*time.to_string() ));

    writer.write_event(Event::Start(testsuite))?;

    for result in results.iter() {

        let mut testcase = BytesStart::borrowed(b"testcase", b"testcase".len());

        testcase.push_attribute(("name", &*result.name));

        if let Some(ref desc) = result.description {
            testcase.push_attribute(("classname", desc as &str));
        } else {
            testcase.push_attribute(("classname", ""));
        }

        testcase.push_attribute(("time", &*result.duration.to_string()));


        writer.write_event(Event::Start(testcase))?;

        writer.write_event(Event::Start(BytesStart::borrowed(b"system-out", b"system-out".len())))?;

        writer.write_event(Event::Text(BytesText::from_plain_str(&filter_invalid_chars(&result.output))))?;

        writer.write_event(Event::End(BytesEnd::borrowed(b"system-out")))?;

        if !result.pass {

            let error_text = result.error.clone().unwrap_or_else(||String::from(""));

            if error_text == "Dependency Not Met" {
                let mut skipped = BytesStart::borrowed(b"skipped", b"skipped".len());
                skipped.push_attribute(("message", "Dependency Not Met"));

                writer.write_event(Event::Start(skipped))?;

                writer.write_event(Event::End(BytesEnd::borrowed(b"skipped")))?;
            } else {
                let mut failure = BytesStart::borrowed(b"failure", b"failure".len());
                failure.push_attribute(("message", &filter_invalid_chars(&error_text) as &str));

                writer.write_event(Event::Start(failure))?;

                writer.write_event(Event::End(BytesEnd::borrowed(b"failure")))?;

            }

        }



        writer.write_event(Event::End(BytesEnd::borrowed(b"testcase")))?;
    }


    writer.write_event(Event::End(BytesEnd::borrowed(b"testsuite")))?;

    Ok(())
}

fn filter_invalid_chars(input: &str) -> String {

    let mut output = String::new();

    for ch in input.chars() {

       if 
         ( ch >= '\u{0020}' && ch <= '\u{D7FF}' ) || 
            ( ch >= '\u{E000}' && ch <= '\u{FFFD}' ) ||
            ch == '\u{0009}' ||
            ch == '\u{0A}' ||
            ch == '\u{0D}' {
                output.push(ch);
            }

    }


    output
}
