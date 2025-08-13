use std::{
    env,
    fmt::Write,
    fs::File,
    io,
    path::{Path, PathBuf},
};

use khttp::{Headers, Method::*, RequestContext, ResponseHandle, Server, Status};

fn main() {
    let args: Vec<String> = env::args().collect();
    let base_dir = args.get(1).map(String::as_str).unwrap_or(".");

    if !Path::new(base_dir).is_dir() {
        eprintln!("error: dir '{}' does not exist", base_dir);
        std::process::exit(1);
    }
    let dir = base_dir.to_string();

    let mut app = Server::builder("127.0.0.1:8080").unwrap();
    app.route(Get, "/**", move |c, r| serve_static_file(&dir, c, r));

    print_startup(base_dir);
    app.build().serve_epoll().unwrap();
}

fn serve_static_file(dir: &str, ctx: RequestContext, res: &mut ResponseHandle) -> io::Result<()> {
    let mut path = ctx.uri.path();
    if path.is_empty() {
        path = "/";
    }

    let full_path = match sanitize_path(dir, path) {
        Some(p) => p,
        None => {
            return res.send(&Status::NOT_FOUND, Headers::empty(), &b"404 Not Found"[..]);
        }
    };

    if full_path.is_dir() {
        let index_path = full_path.join("index.html");
        if index_path.exists() {
            return serve_file(&index_path, res);
        } else {
            return serve_directory_listing(path, &full_path, res);
        }
    }

    serve_file(&full_path, res)
}

fn serve_file(path: &Path, res: &mut ResponseHandle) -> io::Result<()> {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) => {
            let status = match e.kind() {
                io::ErrorKind::NotFound => Status::NOT_FOUND,
                io::ErrorKind::PermissionDenied => Status::FORBIDDEN,
                _ => {
                    eprintln!("error opening file '{}': {}", path.display(), e);
                    Status::INTERNAL_SERVER_ERROR
                }
            };
            return res.send(&status, Headers::empty(), &b"error"[..]);
        }
    };

    let mut headers = Headers::new();
    headers.add(Headers::CONTENT_TYPE, get_mime(path).as_bytes());
    res.ok(&headers, io::BufReader::new(file))
}

fn serve_directory_listing(
    request_path: &str,
    dir_path: &Path,
    res: &mut ResponseHandle,
) -> io::Result<()> {
    let mut html = String::new();

    write!(
        &mut html,
        "<!DOCTYPE html><html><head><meta charset='utf-8'><title>Index of {}</title></head><body>",
        request_path
    )
    .unwrap();

    write!(&mut html, "<h2>Index of {}</h2><ul>", request_path).unwrap();

    // Add parent directory link if not root
    if request_path != "/" {
        let parent = if request_path.ends_with('/') {
            format!("{}..", request_path)
        } else {
            format!("{}/..", request_path)
        };
        write!(&mut html, "<li><a href=\"{}\">..</a></li>", parent).unwrap();
    }

    for entry in dir_path.read_dir()? {
        let entry = entry?;
        let file_name = entry.file_name();
        let file_name_str = file_name.to_string_lossy();

        let is_dir = entry.path().is_dir();
        let display_name = if is_dir {
            format!("{}/", file_name_str)
        } else {
            file_name_str.to_string()
        };

        let full_url = if request_path.ends_with('/') {
            format!("{}{}", request_path, display_name)
        } else {
            format!("{}/{}", request_path, display_name)
        };

        write!(
            &mut html,
            "<li><a href=\"{}\">{}</a></li>",
            full_url, display_name
        )
        .unwrap();
    }

    write!(&mut html, "</ul><hr><em>khttp-static</em></body></html>").unwrap();

    let mut headers = Headers::new();
    headers.add(Headers::CONTENT_TYPE, b"text/html; charset=utf-8");

    res.ok(&headers, html.as_bytes())
}

/// Prevent directory traversal
fn sanitize_path(base: &str, req_path: &str) -> Option<PathBuf> {
    let candidate = Path::new(base).join(req_path.strip_prefix("/").unwrap_or(req_path));
    let canonical = candidate.canonicalize().ok()?;
    let base = Path::new(base).canonicalize().ok()?;
    if canonical.starts_with(&base) {
        Some(canonical)
    } else {
        None
    }
}

fn get_mime(path: &Path) -> &'static str {
    let extension = match path.extension().and_then(|e| e.to_str()) {
        Some(e) => e,
        None => return "text/plain; charset=utf-8",
    };

    match extension {
        "htm" | "html" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" => "application/javascript; charset=utf-8",
        "gif" => "image/gif",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "pdf" => "application/pdf",
        "svg" => "image/svg+xml",
        "json" => "application/json; charset=utf-8",
        "txt" => "text/plain; charset=utf-8",
        _ => "text/plain; charset=utf-8",
    }
}

fn print_startup(base_dir: &str) {
    println!(
        "\n\
         ==========================================\n\
          Serving directory : {}\n\
          Listening on      : http://127.0.0.1:8080\n\
         ==========================================\n",
        base_dir
    );
}
