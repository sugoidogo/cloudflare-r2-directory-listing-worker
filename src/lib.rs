const DATE_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

#[derive(Clone, Eq, Ord, PartialEq, PartialOrd)]
pub enum EntryType {
    Directory,
    File {
        size: u32,
        uploaded: chrono::DateTime<chrono::Utc>,
    },
}

markup::define! {
    EntryList<'a>(
        key_prefix: &'a str,
        readable_key_prefix: &'a str,
        entries: Vec<(EntryType, String)>,
        file_size_format_options: humansize::FormatSizeOptions
    ) {
        @markup::doctype()
        html {
            head {
                meta[charset = "utf-8"] {}
                title { @readable_key_prefix }
                style {
                    "@import url('https://fonts.googleapis.com/css2?family=Inconsolata:wght@300;400;600;700&family=Old+Standard+TT:ital,wght@0,400;0,700;1,400&display=swap');"
                    "html { font-family: 'Inconsolata'; }"
                    "body { padding: 1em; }"
                    "* { margin: 0; padding: 0; }"
                    "header { margin-bottom: 2em; }"
                    "table { margin-left: 1em; }"
                    "td, th { padding: 0.25em; max-width: 300px; }"
                    "thead { background-color: #eee; }"
                    "th { min-width: 100px; font-size: 1.1em; }"
                }
            }
            body {
                header {
                    h1 {
                        @readable_key_prefix
                    }
                }
                table {
                    thead {
                        tr {
                            th { "Name" }
                            th { "Size" }
                            th { "Uploaded" }
                        }
                    }
                    tbody {
                        @if let Some((parent_key, _)) = readable_key_prefix.trim_end_matches('/').rsplit_once('/') {
                            tr {
                                td[colspan = "3"] {
                                    "📁 "
                                    a[href = format!("/{parent_key}/")] {
                                        "../"
                                    }
                                }
                            }
                        }
                        @for (entry_type, key) in entries.into_iter() {
                            tr {
                                @if let EntryType::File { size, uploaded } = entry_type {
                                    td {
                                        "📄 "
                                        a[href = format!("/{key}")] {
                                            @key.strip_prefix(key_prefix).expect("must be a prefix")
                                        }
                                    }
                                    td {
                                        @humansize::format_size(*size, file_size_format_options)
                                    }
                                    td {
                                        @uploaded.format(DATE_FORMAT).to_string()
                                    }
                                } else {
                                    td[colspan = "3"] {
                                        "📁 "
                                        a[href = format!("/{key}")] {
                                            @key.strip_prefix(key_prefix).expect("must be a prefix")
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[worker::event(start)]
pub fn main() {
    console_error_panic_hook::set_once();
}

#[worker::event(fetch)]
pub async fn main(
    request: worker::Request,
    environment: worker::Env,
    _context: worker::Context,
) -> worker::Result<worker::Response> {
    if request.method() != worker::Method::Get {
        return worker::Response::error("Bad Request", 400);
    }

    let bucket = environment.bucket("BUCKET")?;

    let file_size_format_options =
        humansize::FormatSizeOptions::from(humansize::DECIMAL).decimal_places(2);

    let path = request.path();
    let path =
        urlencoding::decode(&path).map_err(|err| worker::Error::RustError(err.to_string()))?;
    let key_prefix = path.trim_start_matches('/');
    let readable_key_prefix = if key_prefix.is_empty() {
        "/"
    } else {
        key_prefix
    };

    if readable_key_prefix.ends_with('/') {
        let list_response = bucket
            .list()
            .delimiter("/")
            .prefix(key_prefix)
            .execute()
            .await?;

        let mut entries: Vec<(EntryType, String)> = list_response
            .delimited_prefixes()
            .into_iter()
            .map(|key| (EntryType::Directory, key))
            .chain(list_response.objects().into_iter().map(|object| {
                (
                    EntryType::File {
                        size: object.size() as u32,
                        uploaded: chrono::NaiveDateTime::from_timestamp_millis(
                            object.uploaded().as_millis() as i64,
                        )
                        .expect("must be valid")
                        .and_utc(),
                    },
                    object.key(),
                )
            }))
            .collect();
        if entries.is_empty() {
            worker::Response::error("Not Found.", 404)
        } else {
            entries.sort();
            let mut headers = worker::Headers::new();
            headers.set("content-type", "text/html")?;
            Ok(worker::Response::ok(
                EntryList {
                    key_prefix,
                    readable_key_prefix,
                    entries,
                    file_size_format_options,
                }
                .to_string(),
            )?
            .with_headers(headers))
        }
    } else {
        match bucket.get(key_prefix).execute().await? {
            Some(object) => {
                worker::Response::from_stream(object.body().expect("must be available").stream()?)
            }
            None => worker::Response::error("Not Found", 404),
        }
    }
}
