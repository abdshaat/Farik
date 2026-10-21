//! The account runner against a fake `claude`: a shell script each test writes into its own
//! directory, which answers `auth status`, `auth login`, `auth logout` and the check from files
//! the test sets, and records what it was given beside itself.
#![cfg(unix)]

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use catervas_core::claude_account::{AccountKind, CHECK_ARGS, CheckOutcome};
use catervas_runtime::account::ClaudeAccount;

const SIGNED_IN: &str =
    r#"{"loggedIn": true, "authMethod": "oauth_token", "apiProvider": "firstParty"}"#;
const SIGNED_OUT: &str = r#"{"loggedIn": false}"#;
const GOOD: &str = r#"{"type":"result","subtype":"success","is_error":false,"result":"OK"}"#;
const REFUSED: &str =
    r#"{"type":"result","is_error":true,"api_error_status":401,"result":"Invalid API key"}"#;

/// A scratch folder with a fake `claude` in it.
struct Fake {
    dir: PathBuf,
}

impl Fake {
    fn new(name: &str) -> Fake {
        let dir =
            std::env::temp_dir().join(format!("catervas-account-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the fake's directory");
        let script = format!(
            "#!/bin/sh\n\
             dir='{dir}'\n\
             case \"$1 $2\" in\n\
             'auth status')\n\
               if [ -f \"$dir/slow\" ]; then sleep 30; fi\n\
               cat \"$dir/status.json\"; exit 0;;\n\
             'auth logout') touch \"$dir/logged_out\"; exit 0;;\n\
             'auth login')\n\
               printf '%s\\n' \"$3\" >> \"$dir/login_args.txt\"\n\
               echo $$ >> \"$dir/login_pids.txt\"\n\
               if [ -f \"$dir/no_address\" ]; then echo 'Opening browser...'; sleep 60; exit 1; fi\n\
               echo \"If the browser didn't open, visit: https://claude.com/cai/oauth/authorize?code=true&client_id=x&state=y\"\n\
               echo 'Paste code here if prompted >'\n\
               IFS= read -r line\n\
               printf '%s\\n' \"$line\" > \"$dir/code.txt\"\n\
               if [ \"$line\" = good-code ]; then exit 0; fi\n\
               echo 'Invalid code'; exit 1;;\n\
             esac\n\
             if [ \"$1\" = \"-p\" ]; then\n\
               printf '%s\\n' \"$@\" >> \"$dir/args.txt\"\n\
               pwd > \"$dir/pwd.txt\"\n\
               cat \"$dir/check.json\"\n\
               case \"$(cat \"$dir/check.json\")\" in *'\"is_error\":true'*) exit 1;; esac\n\
               exit 0\n\
             fi\n\
             exit 2\n",
            dir = dir.display()
        );
        // Written by `cp`, not held open by this process (a forked child would inherit the
        // write handle, and running the file then fails with "text file busy").
        let source = dir.join("claude.txt");
        std::fs::write(&source, script).expect("the script is written");
        let path = dir.join("claude");
        let copied = std::process::Command::new("cp")
            .arg(&source)
            .arg(&path)
            .status()
            .expect("cp runs");
        assert!(copied.success(), "the script is copied");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .expect("the script is executable");
        let fake = Fake { dir };
        fake.set("status.json", SIGNED_IN);
        fake.set("check.json", GOOD);
        fake
    }

    fn set(&self, file: &str, content: &str) {
        std::fs::write(self.dir.join(file), content).expect("the fake's file");
    }

    fn read(&self, file: &str) -> Option<String> {
        std::fs::read_to_string(self.dir.join(file)).ok()
    }

    fn check_dir(&self) -> PathBuf {
        self.dir.join("account-check")
    }

    fn account_with(&self, extra: &[(&str, &str)]) -> ClaudeAccount {
        let mut env = BTreeMap::from([("PATH".to_string(), "/usr/bin:/bin".to_string())]);
        for (name, value) in extra {
            env.insert((*name).to_string(), (*value).to_string());
        }
        ClaudeAccount::new(self.dir.join("claude"), env, self.check_dir())
    }

    fn account(&self) -> ClaudeAccount {
        self.account_with(&[])
    }
}

impl Drop for Fake {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[tokio::test]
#[ignore = "needs a shell: cargo xtask check --integration"]
async fn asks_claude_code_and_checks_with_one_tiny_request() {
    let fake = Fake::new("check");
    let account = fake.account();
    assert_eq!(account.check().await, CheckOutcome::Working);
    let args = fake.read("args.txt").expect("the request was sent");
    let given: Vec<&str> = args.lines().collect();
    assert_eq!(given, CHECK_ARGS.to_vec());
    let ran_in = fake.read("pwd.txt").expect("the folder was recorded");
    assert_eq!(
        Path::new(ran_in.trim()).canonicalize().expect("it exists"),
        fake.check_dir()
            .canonicalize()
            .expect("the folder was made")
    );

    // Not signed in: the request is not sent.
    let signed_out = Fake::new("check-out");
    signed_out.set("status.json", SIGNED_OUT);
    assert_eq!(
        signed_out.account().check().await,
        CheckOutcome::NotSignedIn
    );
    assert!(signed_out.read("args.txt").is_none());

    // A refused key.
    fake.set("check.json", REFUSED);
    assert_eq!(account.check().await, CheckOutcome::KeyRefused);

    // A status that never answers is not signed in, within its limit.
    let slow = Fake::new("slow");
    slow.set("slow", "");
    let started = Instant::now();
    let status = slow.account().status().await;
    assert!(!status.signed_in);
    assert_eq!(status.kind, None);
    assert!(started.elapsed() < Duration::from_secs(20));

    // A key in the user's environment is named, never read.
    let status = fake
        .account_with(&[("ANTHROPIC_API_KEY", "x")])
        .status()
        .await;
    assert!(status.signed_in);
    assert_eq!(status.kind, Some(AccountKind::Environment));
    let plain = account.status().await;
    assert_eq!(plain.kind, Some(AccountKind::Subscription));

    // Signing out is Claude Code's logout.
    assert_eq!(account.sign_out().await, Ok(()));
    assert!(fake.read("logged_out").is_some());
}
