#![allow(dead_code)]
use serde_json::Value;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
static NEXT: AtomicUsize = AtomicUsize::new(0);
pub struct Temp(pub PathBuf);
impl Temp {
    pub fn new() -> Self {
        let p = std::env::temp_dir()
            .join("impro-gitviews-test-repos")
            .join(format!(
                "{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(&p).unwrap();
        Self(p)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
/// The null device as git understands it (`GIT_CONFIG_GLOBAL` needs an existing, empty file).
pub const NULL_FILE: &str = if cfg!(windows) { "NUL" } else { "/dev/null" };
/// A path for `core.hooksPath` that holds no hooks: `/dev/null` on Unix, an empty directory
/// elsewhere (a device name is not a directory on Windows).
pub fn no_hooks_path() -> String {
    if cfg!(windows) {
        let dir = std::env::temp_dir()
            .join("impro-gitviews-test-repos")
            .join("no-hooks");
        fs::create_dir_all(&dir).unwrap();
        dir.to_string_lossy().replace('\\', "/")
    } else {
        NULL_FILE.to_owned()
    }
}
pub struct GitRepo {
    pub path: PathBuf,
    pub tick: u64,
}
impl GitRepo {
    pub fn new(path: &Path) -> Self {
        fs::create_dir_all(path).unwrap();
        let r = Self {
            path: path.to_owned(),
            tick: 0,
        };
        r.git(&["init", "--initial-branch=master", "."]);
        for (k, v) in [
            ("user.name", "Ada Example"),
            ("user.email", "ada@example.test"),
            ("commit.gpgsign", "false"),
            ("core.autocrlf", "false"),
        ] {
            r.git(&["config", k, v]);
        }
        r
    }
    pub fn command(&self, args: &[&str], input: Option<&[u8]>, env: &[(&str, String)]) -> String {
        let hooks = format!("core.hooksPath={}", no_hooks_path());
        let mut cmd = Command::new("git");
        for (k, _) in std::env::vars().filter(|(k, _)| k.starts_with("GIT_")) {
            cmd.env_remove(k);
        }
        cmd.args([
            "-c",
            &hooks,
            "-c",
            "commit.gpgsign=false",
            "-c",
            "tag.gpgsign=false",
            "-c",
            "init.defaultBranch=master",
            "-C",
        ])
        .arg(&self.path)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", NULL_FILE)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_AUTHOR_NAME", "Ada Example")
        .env("GIT_AUTHOR_EMAIL", "ada@example.test")
        .env("GIT_COMMITTER_NAME", "Grace Example")
        .env("GIT_COMMITTER_EMAIL", "grace@example.test")
        .env("LC_ALL", "C")
        .env("GIT_DEFAULT_HASH", "sha1")
        .envs(env.iter().cloned())
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
        let mut child = cmd.spawn().unwrap();
        if let Some(data) = input {
            child.stdin.take().unwrap().write_all(data).unwrap();
        }
        let out = child.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }
    pub fn git(&self, args: &[&str]) -> String {
        self.command(args, None, &[])
    }
    pub fn dated(&mut self, date: Option<&str>) -> Vec<(&'static str, String)> {
        let stamp = date
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{} +0000", 1735732800 + self.tick * 30 * 3600));
        self.tick += 1;
        vec![
            ("GIT_AUTHOR_DATE", stamp.clone()),
            ("GIT_COMMITTER_DATE", stamp),
        ]
    }
    pub fn commit(&mut self, files: &[(&str, Option<&[u8]>)], message: &str) -> String {
        self.commit_as(files, message, None, None)
    }
    pub fn commit_as(
        &mut self,
        files: &[(&str, Option<&[u8]>)],
        message: &str,
        date: Option<&str>,
        author: Option<(&str, &str)>,
    ) -> String {
        for (name, value) in files {
            let path = self.path.join(name);
            match value {
                Some(data) => {
                    fs::create_dir_all(path.parent().unwrap()).unwrap();
                    if path.is_symlink() {
                        fs::remove_file(&path).unwrap();
                    }
                    fs::write(path, data).unwrap();
                }
                None => {
                    if path.is_dir() && !path.is_symlink() {
                        fs::remove_dir_all(path).unwrap();
                    } else if path.exists() || path.is_symlink() {
                        fs::remove_file(path).unwrap();
                    }
                }
            }
        }
        self.git(&["add", "--all"]);
        let mut env = self.dated(date);
        if let Some((name, email)) = author {
            env.extend([
                ("GIT_AUTHOR_NAME", name.to_owned()),
                ("GIT_AUTHOR_EMAIL", email.to_owned()),
            ]);
        }
        self.command(
            &["commit", "--allow-empty", "-F", "-"],
            Some(message.as_bytes()),
            &env,
        );
        self.git(&["rev-parse", "HEAD"])
    }
    pub fn branch(&self, name: &str) {
        self.git(&["branch", name]);
    }
    pub fn checkout(&self, name: &str, create: bool) {
        if create {
            self.git(&["checkout", "-b", name]);
        } else {
            self.git(&["checkout", name]);
        }
    }
    pub fn merge(&mut self, branches: &[&str], message: &str) {
        let mut args = vec!["merge", "--no-ff", "-m", message];
        args.extend(branches);
        let env = self.dated(None);
        self.command(&args, None, &env);
    }
    pub fn tag(&mut self, name: &str, annotated: bool) {
        let env = self.dated(None);
        let args = if annotated {
            vec!["tag", "-a", name, "-m", name]
        } else {
            vec!["tag", name]
        };
        self.command(&args, None, &env);
    }
}
pub fn linear(path: &Path, count: usize) -> GitRepo {
    let mut r = GitRepo::new(path);
    for i in 0..count {
        r.commit(
            &[(
                "main.py",
                Some(format!("def value():\n    return {i}\n").as_bytes()),
            )],
            &format!("Linear commit {}", i + 1),
        );
    }
    r
}
pub fn octopus(path: &Path) -> GitRepo {
    let mut r = linear(path, 2);
    for name in ["feature/auth", "feature/api"] {
        r.checkout(name, true);
        r.commit(
            &[(&format!("{name}.py"), Some(b"ENABLED = True\n"))],
            &format!("Add {name}"),
        );
        r.checkout("master", false);
    }
    r.merge(
        &["feature/auth", "feature/api"],
        "Merge pull request #42 from acme/feature/auth",
    );
    r
}
pub fn detached(path: &Path) -> GitRepo {
    let mut r = linear(path, 5);
    r.git(&["checkout", "--detach", "HEAD~1"]);
    r.commit(
        &[("detached.txt", Some(b"Only reachable from HEAD\n"))],
        "Detached work",
    );
    r
}
pub fn clone_with_remote(path: &Path) -> GitRepo {
    let mut source = linear(&path.join("source"), 3);
    source.tag("v1", true);
    source.git(&[
        "clone",
        "--no-local",
        source.path.to_str().unwrap(),
        path.join("clone").to_str().unwrap(),
    ]);
    GitRepo {
        path: path.join("clone"),
        tick: 0,
    }
}
pub fn deleted_branches(path: &Path) -> GitRepo {
    let mut r = linear(path, 2);
    for slug in ["pane-split", "explorer-no-badges", "git-data"] {
        let name = format!("ceo/codex/{slug}");
        r.checkout(&name, true);
        r.commit(
            &[(&format!("{slug}.py"), Some(b"VALUE = \"side branch\"\n"))],
            &format!("Implement {slug}"),
        );
        r.checkout("master", false);
        r.merge(&[&name], &format!("Merge {name}: implementation and tests"));
        r.git(&["branch", "-D", &name]);
    }
    r
}
pub fn fast_history(path: &Path, count: usize) -> GitRepo {
    let r = GitRepo::new(path);
    let mut commands = Vec::with_capacity(count * 220);
    commands.extend_from_slice(b"blob\nmark :1\ndata 2\nx\n\n");
    for i in 0..count {
        let message = format!("Generated commit {i}");
        write!(commands,"commit refs/heads/master\nmark :{}\nauthor Fast <fast@example.test> {} +0530\ncommitter Fast <fast@example.test> {} +0000\ndata {}\n{}\n",i+2,1735732800+i,1735732800+i,message.len(),message).unwrap();
        if i > 0 {
            writeln!(commands, "from :{}", i + 1).unwrap();
        }
        commands.extend_from_slice(b"M 100644 :1 file.txt\n\n");
    }
    r.command(&["fast-import", "--quiet"], Some(&commands), &[]);
    r
}
pub fn topological(graph: &Value) {
    let commits = graph["commits"].as_array().unwrap();
    let positions: std::collections::HashMap<_, _> = commits
        .iter()
        .enumerate()
        .map(|(i, c)| (c["hash"].as_str().unwrap(), i))
        .collect();
    for (i, c) in commits.iter().enumerate() {
        let sha = c["hash"].as_str().unwrap();
        assert_eq!(sha.len(), 40);
        assert!(sha
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
        for p in c["parents"].as_array().unwrap() {
            if let Some(j) = positions.get(p.as_str().unwrap()) {
                assert!(i < *j);
            }
        }
    }
}
pub fn strings(values: &Value, key: &str) -> std::collections::BTreeSet<String> {
    values
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v[key].as_str().unwrap().to_owned())
        .collect()
}
/// Run a fault-injection test in its own process so PATH changes never race tests.
pub fn isolated(name: &str, body: impl FnOnce(&Path)) {
    if let Some(path) = std::env::var_os("IMPRO_TEST_FAULT_ROOT") {
        std::env::set_var(
            "GIT_CEILING_DIRECTORIES",
            Path::new(&path).parent().unwrap(),
        );
        body(Path::new(&path));
        return;
    }
    let temp = Temp::new();
    let out = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env("IMPRO_TEST_FAULT_ROOT", &temp.0)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
pub fn executable(path: &Path, contents: &str) {
    fs::write(path, contents).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
}

pub fn showcase(path: &Path) -> GitRepo {
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    let mut r = GitRepo::new(path);
    r.commit(&[("app.py",Some("#!/usr/bin/env python3\n\"\"\"A small demo application.\n\nThe graph shows the story behind each change.\n\"\"\"\n\ndef greet(name):\n    return f\"Hello, {name}\"\n\n# context line 0\n# context line 1\n# context line 2\n# context line 3\n# context line 4\n# context line 5\n# context line 6\n# context line 7\n# context line 8\n# context line 9\n# context line 10\n# context line 11\n# context line 12\n# context line 13\n# context line 14\n# context line 15\n# context line 16\n# context line 17\n# context line 18\n# context line 19\n# context line 20\n# context line 21\n# context line 22\n# context line 23\n\ndef main():\n    print(greet(\"world\"))\n\nmain()\n".as_bytes())),("README.md",Some("# Acme workspace\n\nA graph demo.\n".as_bytes())),("obsolete.txt",Some("Remove this after the migration.\n".as_bytes())),("empty.txt",Some("".as_bytes())),("docs/Привет мир.md",Some("# Привет, мир 🌍\n".as_bytes())),("no-newline.txt",Some("last line".as_bytes())),("windows.txt",Some(&[102,105,114,115,116,13,10,115,101,99,111,110,100,13,10])),("public/icon.png",Some(&[137,80,78,71,13,10,26,10,0,0,0,13,73,72,68,82,0,0,0,1])),("package.json",Some("{\"name\": \"acme\", \"version\": \"0.1.0\"}\n".as_bytes()))],"Create workspace");
    r.commit(
        &[(
            "README.md",
            Some("# Acme workspace\n\nExplore commits, branches and changes.\n".as_bytes()),
        )],
        "Document the project",
    );
    r.tag("v0.1", false);
    r.branch("feature/auth");
    r.commit(
        &[(
            "config.json",
            Some("{\n  \"theme\": \"dark\",\n  \"graph\": true\n}\n".as_bytes()),
        )],
        "Configure graph defaults",
    );
    r.branch("feature/api");
    r.commit(&[("app.py",Some("#!/usr/bin/env python3\n\"\"\"A small demo application.\n\nThe graph shows the story behind each change.\n\"\"\"\n\ndef greet(name):\n    return f\"Welcome, {name}\"\n\n# context line 0\n# context line 1\n# context line 2\n# context line 3\n# context line 4\n# context line 5\n# context line 6\n# context line 7\n# context line 8\n# context line 9\n# context line 10\n# context line 11\n# context line 12\n# context line 13\n# context line 14\n# context line 15\n# context line 16\n# context line 17\n# context line 18\n# context line 19\n# context line 20\n# context line 21\n# context line 22\n# context line 23\n\ndef main():\n    print(greet(\"Acme\"))\n\nmain()\n".as_bytes()))],"Improve greetings and startup");
    r.checkout("feature/auth", false);
    r.commit(&[("auth.py",Some("\"\"\"Authentication helpers.\nSession implementation 0.\n\"\"\"\n\ndef allowed(user):\n    return bool(user)\n".as_bytes()))],"Add sign in");
    r.commit(&[("auth.py",Some("\"\"\"Authentication helpers.\nSession implementation 1.\n\"\"\"\n\ndef allowed(user):\n    return bool(user)\n".as_bytes()))],"Validate sessions");
    r.commit(&[("auth.py",Some("\"\"\"Authentication helpers.\nSession implementation 2.\n\"\"\"\n\ndef allowed(user):\n    return bool(user)\n".as_bytes()))],"Add access checks");
    r.checkout("feature/api", false);
    r.commit(&[("src/api.js",Some("// API client revision 0\nexport async function fetchItems() {\n  return fetch(\"/api/items\");\n}\n".as_bytes()))],"Create API client");
    r.commit(&[("src/api.js",Some("// API client revision 1\nexport async function fetchItems() {\n  return fetch(\"/api/items\");\n}\n".as_bytes()))],"Add retries");
    r.commit(&[("src/api.js",Some("// API client revision 2\nexport async function fetchItems() {\n  return fetch(\"/api/items\");\n}\n".as_bytes()))],"Handle API errors");
    r.commit(&[("src/api.js",Some("// API client revision 3\nexport async function fetchItems() {\n  return fetch(\"/api/items\");\n}\n".as_bytes()))],"Document endpoints");
    r.checkout("master", false);
    r.commit(&[("obsolete.txt",None),("docs/guide.md",Some("# Migration\n\nUse the new client.\nExplore branches and inspect commits in the graph.\nThe commit tab shows source changes and syntax colours.\n".as_bytes()))],"Remove legacy setup");
    r.merge(
        &["feature/auth", "feature/api"],
        "Merge pull request #42 from acme/feature/auth",
    );
    #[cfg(unix)]
    std::fs::set_permissions(
        r.path.join("app.py"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    r.commit(
        &[(
            "app.py",
            Some(&[
                35, 33, 47, 117, 115, 114, 47, 98, 105, 110, 47, 101, 110, 118, 32, 112, 121, 116,
                104, 111, 110, 51, 10, 34, 34, 34, 65, 32, 115, 109, 97, 108, 108, 32, 100, 101,
                109, 111, 32, 97, 112, 112, 108, 105, 99, 97, 116, 105, 111, 110, 46, 10, 10, 84,
                104, 101, 32, 103, 114, 97, 112, 104, 32, 115, 104, 111, 119, 115, 32, 116, 104,
                101, 32, 115, 116, 111, 114, 121, 32, 98, 101, 104, 105, 110, 100, 32, 101, 97, 99,
                104, 32, 99, 104, 97, 110, 103, 101, 46, 10, 34, 34, 34, 10, 10, 100, 101, 102, 32,
                103, 114, 101, 101, 116, 40, 110, 97, 109, 101, 41, 58, 10, 32, 32, 32, 32, 114,
                101, 116, 117, 114, 110, 32, 102, 34, 87, 101, 108, 99, 111, 109, 101, 44, 32, 123,
                110, 97, 109, 101, 125, 34, 10, 10, 35, 32, 99, 111, 110, 116, 101, 120, 116, 32,
                108, 105, 110, 101, 32, 48, 10, 35, 32, 99, 111, 110, 116, 101, 120, 116, 32, 108,
                105, 110, 101, 32, 49, 10, 35, 32, 99, 111, 110, 116, 101, 120, 116, 32, 108, 105,
                110, 101, 32, 50, 10, 35, 32, 99, 111, 110, 116, 101, 120, 116, 32, 108, 105, 110,
                101, 32, 51, 10, 35, 32, 99, 111, 110, 116, 101, 120, 116, 32, 108, 105, 110, 101,
                32, 52, 10, 35, 32, 99, 111, 110, 116, 101, 120, 116, 32, 108, 105, 110, 101, 32,
                53, 10, 35, 32, 99, 111, 110, 116, 101, 120, 116, 32, 108, 105, 110, 101, 32, 54,
                10, 35, 32, 99, 111, 110, 116, 101, 120, 116, 32, 108, 105, 110, 101, 32, 55, 10,
                35, 32, 99, 111, 110, 116, 101, 120, 116, 32, 108, 105, 110, 101, 32, 56, 10, 35,
                32, 99, 111, 110, 116, 101, 120, 116, 32, 108, 105, 110, 101, 32, 57, 10, 35, 32,
                99, 111, 110, 116, 101, 120, 116, 32, 108, 105, 110, 101, 32, 49, 48, 10, 35, 32,
                99, 111, 110, 116, 101, 120, 116, 32, 108, 105, 110, 101, 32, 49, 49, 10, 35, 32,
                99, 111, 110, 116, 101, 120, 116, 32, 108, 105, 110, 101, 32, 49, 50, 10, 35, 32,
                99, 111, 110, 116, 101, 120, 116, 32, 108, 105, 110, 101, 32, 49, 51, 10, 35, 32,
                99, 111, 110, 116, 101, 120, 116, 32, 108, 105, 110, 101, 32, 49, 52, 10, 35, 32,
                99, 111, 110, 116, 101, 120, 116, 32, 108, 105, 110, 101, 32, 49, 53, 10, 35, 32,
                99, 111, 110, 116, 101, 120, 116, 32, 108, 105, 110, 101, 32, 49, 54, 10, 35, 32,
                99, 111, 110, 116, 101, 120, 116, 32, 108, 105, 110, 101, 32, 49, 55, 10, 35, 32,
                99, 111, 110, 116, 101, 120, 116, 32, 108, 105, 110, 101, 32, 49, 56, 10, 35, 32,
                99, 111, 110, 116, 101, 120, 116, 32, 108, 105, 110, 101, 32, 49, 57, 10, 35, 32,
                99, 111, 110, 116, 101, 120, 116, 32, 108, 105, 110, 101, 32, 50, 48, 10, 35, 32,
                99, 111, 110, 116, 101, 120, 116, 32, 108, 105, 110, 101, 32, 50, 49, 10, 35, 32,
                99, 111, 110, 116, 101, 120, 116, 32, 108, 105, 110, 101, 32, 50, 50, 10, 35, 32,
                99, 111, 110, 116, 101, 120, 116, 32, 108, 105, 110, 101, 32, 50, 51, 10, 10, 100,
                101, 102, 32, 109, 97, 105, 110, 40, 41, 58, 10, 32, 32, 32, 32, 112, 114, 105,
                110, 116, 40, 103, 114, 101, 101, 116, 40, 34, 65, 99, 109, 101, 34, 41, 41, 10,
                10, 109, 97, 105, 110, 40, 41, 10,
            ]),
        )],
        "Make the launcher executable",
    );
    r.branch("feature/ui");
    r.commit(
        &[
            ("no-newline.txt", Some("still no final newline".as_bytes())),
            (
                "windows.txt",
                Some(&[
                    102, 105, 114, 115, 116, 13, 10, 117, 112, 100, 97, 116, 101, 100, 13, 10, 116,
                    104, 105, 114, 100, 13, 10,
                ]),
            ),
        ],
        "Support mixed line endings",
    );
    r.checkout("feature/ui", false);
    r.commit(
        &[
            (
                "src/graph.css",
                Some(
                    ":root { --accent: #7c8cff; }\n.commit { padding: 8px; color: white; }\n"
                        .as_bytes(),
                ),
            ),
            ("src/view.js", Some("export const zoom = 1;\n".as_bytes())),
        ],
        "Add graph palette",
    );
    r.commit(
        &[
            (
                "src/graph.css",
                Some(
                    ":root { --accent: #7c8cff; }\n.commit { padding: 9px; color: white; }\n"
                        .as_bytes(),
                ),
            ),
            ("src/view.js", Some("export const zoom = 2;\n".as_bytes())),
        ],
        "Style commit cards",
    );
    r.commit(
        &[
            (
                "src/graph.css",
                Some(
                    ":root { --accent: #7c8cff; }\n.commit { padding: 10px; color: white; }\n"
                        .as_bytes(),
                ),
            ),
            ("src/view.js", Some("export const zoom = 3;\n".as_bytes())),
        ],
        "Polish graph controls",
    );
    r.checkout("master", false);
    r.branch("hotfix/critical");
    r.merge(&["feature/ui"], "Merge branch 'feature/ui'");
    r.tag("v0.2", true);
    r.commit(&[("docs/guide.md",None),("docs/getting-started.md",Some("# Migration\n\nUse the new client.\nExplore branches and inspect commits in the graph.\nThe commit tab shows source changes and syntax colours.\n\nStart with the graph view.\n".as_bytes()))],"Rename the getting started guide");
    r.commit(
        &[(
            "package.json",
            Some("{\"name\": \"acme\", \"version\": \"0.2.0\"}\n".as_bytes()),
        )],
        "Publish package metadata",
    );
    r.branch("experiment/graph-polish");
    r.commit(&[("src/layout.json",Some("{\"lanes\": \"compact\", \"note\": \"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\"}\n".as_bytes()))],"Tune graph spacing");
    r.checkout("hotfix/critical", false);
    r.commit(
        &[(
            "reliability.py",
            Some(
                "\"\"\"Bound network work.\"\"\"\nTIMEOUT = 5\n\ndef ready():\n    return True\n"
                    .as_bytes(),
            ),
        )],
        "Reproduce timeout",
    );
    r.commit(
        &[(
            "reliability.py",
            Some(
                "\"\"\"Bound network work.\"\"\"\nTIMEOUT = 4\n\ndef ready():\n    return True\n"
                    .as_bytes(),
            ),
        )],
        "Bound worker requests",
    );
    r.commit(
        &[(
            "reliability.py",
            Some(
                "\"\"\"Bound network work.\"\"\"\nTIMEOUT = 3\n\ndef ready():\n    return True\n"
                    .as_bytes(),
            ),
        )],
        "Handle cancellation",
    );
    r.commit(
        &[(
            "reliability.py",
            Some(
                "\"\"\"Bound network work.\"\"\"\nTIMEOUT = 2\n\ndef ready():\n    return True\n"
                    .as_bytes(),
            ),
        )],
        "Cover timeout regression",
    );
    r.checkout("master", false);
    r.merge(
        &["hotfix/critical"],
        "Merge pull request #47 from acme/hotfix/critical",
    );
    r.commit(
        &[(
            "README.md",
            Some("# Acme workspace\n\nVersion 0.2 is ready.\n".as_bytes()),
        )],
        "Prepare the release",
    );
    r.checkout("experiment/graph-polish", false);
    r.commit(
        &[(
            "src/experiment.js",
            Some("export const smoothLanes = 1;\n".as_bytes()),
        )],
        "Experiment with graph polish 1",
    );
    r.commit(
        &[(
            "src/experiment.js",
            Some("export const smoothLanes = 2;\n".as_bytes()),
        )],
        "Experiment with graph polish 2",
    );
    r.checkout("master", false);
    std::fs::OpenOptions::new()
        .append(true)
        .open(r.path.join("README.md"))
        .unwrap()
        .write_all("\nUncommitted release notes.\n".as_bytes())
        .unwrap();
    std::fs::OpenOptions::new()
        .append(true)
        .open(r.path.join("app.py"))
        .unwrap()
        .write_all("\n# Work in progress\n".as_bytes())
        .unwrap();
    r
}
