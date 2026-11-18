use super::*;

fn os(words: &[&str]) -> Vec<OsString> {
    words.iter().map(OsString::from).collect()
}

#[test]
fn variables_are_read_and_written_back() {
    let env = parse_env("MANGOHUD=1  DRI_PRIME=1\nNAME=\"two words\" EMPTY=").unwrap();
    assert_eq!(env["MANGOHUD"], "1");
    assert_eq!(env["NAME"], "two words");
    assert_eq!(env["EMPTY"], "");
    assert_eq!(parse_env(&env_line(&env)).unwrap(), env);
    assert_eq!(
        env_line(&env),
        "DRI_PRIME=1 EMPTY=\"\" MANGOHUD=1 NAME=\"two words\""
    );
    assert!(parse_env("JUSTANAME").unwrap_err().contains("NAME=value"));
    assert!(parse_env("1X=2")
        .unwrap_err()
        .contains("letters, digits and _"));
    assert!(parse_env("").unwrap().is_empty());
}

#[test]
fn the_java_heap_slider_reaches_the_computers_memory() {
    assert_eq!(memory_at(1., 16384), 16384);
    assert_eq!(memory_at(0., 16384), 1);
    assert_eq!(memory_at(0.25, 16384), 4096);
    assert_eq!(memory_at(0.5, 8000), 4032);
    assert_eq!(memory_fraction(16384, 16384), 1.);
    assert_eq!(memory_fraction(4096, 16384), 0.25);
    // Saved on a computer with more memory.
    assert_eq!(memory_fraction(32768, 16384), 1.);
}

#[test]
fn a_typed_memory_limit_is_read_in_mb_or_gb_with_no_lowest_value() {
    assert_eq!(parse_memory("2048"), Ok(Some(2048)));
    assert_eq!(parse_memory(" 512 MB "), Ok(Some(512)));
    assert_eq!(parse_memory("4gb"), Ok(Some(4096)));
    assert_eq!(parse_memory("2 G"), Ok(Some(2048)));
    assert_eq!(parse_memory("1"), Ok(Some(1)));
    assert_eq!(parse_memory(""), Ok(None));
    assert_eq!(parse_memory("off"), Ok(None));
    for bad in ["lots", "-5", "1.5gb", "4 tb", "99999999999", "9999999gb"] {
        assert_eq!(parse_memory(bad), Err(NotMemory), "{bad}");
    }
}

#[test]
fn a_summary_says_what_runs() {
    assert_eq!(Launch::default().summary(), None);
    let launch = Launch {
        pre_launch: "./sync.sh".into(),
        wrapper: "nice -n 5".into(),
        env: parse_env("A=1 B=2").unwrap(),
        ..Launch::default()
    };
    assert_eq!(
        launch.summary().unwrap(),
        "runs ./sync.sh first, through nice -n 5, with 2 variables"
    );
}

#[test]
fn each_system_gets_its_plan() {
    let game = os(&["/opt/sunrise-client", "--world", "My World"]);
    let wrapper = Launch {
        wrapper: "nice -n 5".into(),
        ..Launch::default()
    };
    assert_eq!(
        plan(&wrapper, game.clone(), false, &Found::default()),
        Plan::Direct(os(&[
            "nice",
            "-n",
            "5",
            "/opt/sunrise-client",
            "--world",
            "My World"
        ]))
    );
    let both = Launch {
        pre_launch: "echo hi".into(),
        ..wrapper.clone()
    };
    let Plan::Shell { script, words } = plan(&both, game.clone(), false, &Found::default()) else {
        panic!()
    };
    assert!(script.starts_with("(\necho hi\n) || "));
    assert!(script.ends_with("\nexec \"$@\""));
    assert_eq!(words[0], "nice");
    assert_eq!(
        plan(
            &both,
            os(&[r"C:\Sunrise\sunrise-client.exe", "--world", "My World"]),
            true,
            &Found::default()
        ),
        Plan::Cmd {
            line: r#"echo hi && nice -n 5 C:\Sunrise\sunrise-client.exe --world ^"My World^""#
                .into()
        }
    );
}

#[test]
fn windows_words_are_quoted_as_the_c_runtime_reads_them() {
    let q = |w: &str| windows_quote(OsStr::new(w));
    assert_eq!(q("plain"), "plain");
    assert_eq!(q("two words"), "\"two words\"");
    assert_eq!(q(""), "\"\"");
    assert_eq!(q(r#"say "hi""#), r#""say \"hi\"""#);
    assert_eq!(q(r"C:\My Folder\"), r#""C:\My Folder\\""#);
}

/// The characters cmd.exe acts on as operators in `line`: outside quotes and not after a caret.
fn cmd_operators(line: &str) -> String {
    let (mut out, mut quoted, mut chars) = (String::new(), false, line.chars());
    while let Some(c) = chars.next() {
        match c {
            '"' => quoted = !quoted,
            '^' if !quoted => {
                chars.next();
            }
            '&' | '|' | '<' | '>' | '(' | ')' if !quoted => out.push(c),
            _ => {}
        }
    }
    out
}

#[test]
fn windows_game_words_are_escaped_for_cmd_after_a_pre_launch_command() {
    let pre = "echo %USERNAME% & echo hi";
    let launch = Launch {
        pre_launch: pre.into(),
        ..Launch::default()
    };
    let line = |words: &[&str]| {
        let Plan::Cmd { line } = plan(&launch, os(words), true, &Found::default()) else {
            panic!()
        };
        // The player's own text stays as typed.
        let Some(game) = line.strip_prefix(&format!("{pre} && ")) else {
            panic!("{line}")
        };
        game.to_owned()
    };
    assert_eq!(
        line(&["game.exe", "--server", "a&calc"]),
        "game.exe --server a^&calc"
    );
    assert_eq!(cmd_operators(&line(&["game.exe", r#"x" & calc & ""#])), "");
    assert_eq!(line(&["game.exe", "%PATH%"]), "game.exe ^%PATH^%");
    // cmd removes the carets, so the game reads `"My World"`: one argument.
    assert_eq!(
        line(&["game.exe", "--world", "My World"]),
        r#"game.exe --world ^"My World^""#
    );
    // cmd finds the program by its own quotes; escaped, they would split the path at the space.
    assert_eq!(
        line(&[r"C:\Program Files (x86)\game.exe"]),
        r#""C:\Program Files (x86)\game.exe""#
    );
    // `^!` keeps `!VAR!` as it is only with delayed expansion off, which the registry can turn on.
    #[cfg(windows)]
    {
        let cmd = wrap(Command::new("game.exe"), &launch);
        let args = cmd.get_args().collect::<Vec<_>>();
        assert!(
            args[0].to_string_lossy().starts_with("/D /V:OFF /S /C \""),
            "{args:?}"
        );
    }
    // Without cmd the words are passed on as they are.
    let words = os(&["game.exe", "--server", "a&calc"]);
    assert_eq!(
        plan(&Launch::default(), words.clone(), true, &Found::default()),
        Plan::Direct(words.clone())
    );
    assert_eq!(
        plan(&launch, words.clone(), false, &Found::default()),
        Plan::Shell {
            script: script(pre),
            words
        }
    );
}

#[test]
fn nothing_to_add_leaves_the_command_as_it_is() {
    let mut cmd = Command::new("/opt/game");
    cmd.arg("--x").env("A", "1");
    let out = wrap(cmd, &Launch::default());
    assert_eq!(out.get_program(), "/opt/game");
    assert_eq!(out.get_args().collect::<Vec<_>>(), ["--x"]);
}

#[cfg(unix)]
#[test]
fn the_wrapped_game_runs_after_its_pre_launch_command_in_its_folder() {
    let dir = std::env::temp_dir().join(format!("sunrise-launch-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut game = Command::new("/bin/sh");
    // The "game" prints its words and two variables: one from the launcher, one the instance's.
    game.args([
        "-c",
        "echo \"$1|$2|$FROM_LAUNCHER|$FROM_INSTANCE\"",
        "game",
        "a b",
        "$HOME",
    ])
    .current_dir(&dir)
    .env("FROM_LAUNCHER", "yes");
    let launch = Launch {
        pre_launch: "echo ran > pre.txt".into(),
        wrapper: "/usr/bin/env WRAPPED=1".into(),
        env: parse_env("FROM_INSTANCE=\"it works\"").unwrap(),
        ..Launch::default()
    };
    let out = wrap(game, &launch).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "a b|$HOME|yes|it works\n"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("pre.txt")).unwrap(),
        "ran\n"
    );

    let failing = Launch {
        pre_launch: "exit 3".into(),
        ..Launch::default()
    };
    let mut game = Command::new("/bin/sh");
    game.args(["-c", "echo started > started.txt"])
        .current_dir(&dir);
    let out = wrap(game, &failing).output().unwrap();
    assert_eq!(out.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&out.stderr).contains("pre-launch command failed (exit 3)"));
    assert!(!dir.join("started.txt").exists());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_linux_switches_act_only_where_their_tools_are() {
    let all = Found {
        gamemode: true,
        mangohud: true,
        two_gpus: true,
        nvidia: false,
    };
    let launch = Launch {
        wrapper: "nice".into(),
        gamemode: true,
        mangohud: true,
        dedicated_gpu: true,
        ..Launch::default()
    };
    assert_eq!(
        plan(&launch, os(&["game"]), false, &all),
        Plan::Direct(os(&["gamemoderun", "mangohud", "nice", "game"]))
    );
    assert_eq!(switch_env(&launch, &all), [("DRI_PRIME", "1")]);
    let nvidia = Found {
        nvidia: true,
        ..all
    };
    assert_eq!(
        switch_env(&launch, &nvidia),
        [
            ("__NV_PRIME_RENDER_OFFLOAD", "1"),
            ("__VK_LAYER_NV_optimus", "NVIDIA_only"),
            ("__GLX_VENDOR_LIBRARY_NAME", "nvidia")
        ]
    );
    // Not installed (or not Linux): switched on, but nothing is added.
    let none = Found::default();
    assert_eq!(
        plan(&launch, os(&["game"]), false, &none),
        Plan::Direct(os(&["nice", "game"]))
    );
    assert!(switch_env(&launch, &none).is_empty());
    assert!(!none.any() && all.any());
    assert_eq!(
        launch.summary().unwrap(),
        "through nice, with GameMode, with MangoHud, on the dedicated GPU"
    );
    // Off switches aren't written.
    let text = serde_json::to_string(&Launch::default()).unwrap();
    assert_eq!(text, "{}");
}

#[test]
fn the_post_exit_command_runs_with_the_instance_variables() {
    let dir = std::env::temp_dir().join(format!("sunrise-post-exit-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    assert!(after_exit(&Launch::default(), &dir, &[]).is_none());
    let launch = Launch {
        post_exit: if cfg!(windows) {
            // The redirection first: `1> after.txt` would redirect stdout and drop the 1.
            "> after.txt echo %INST_NAME% %INST_ID% %MINE%".into()
        } else {
            "echo \"$INST_NAME $INST_ID $MINE\" > after.txt".into()
        },
        env: parse_env("MINE=1").unwrap(),
        ..Launch::default()
    };
    assert_eq!(
        launch.summary().unwrap(),
        format!("runs {} after, with 1 variable", launch.post_exit)
    );
    let vars = instance_vars("My Pack", "my-pack", &dir);
    let names: Vec<&str> = vars.iter().map(|(k, _)| *k).collect();
    assert_eq!(names, ["INST_NAME", "INST_ID", "INST_DIR", "INST_MC_DIR"]);
    assert!(after_exit(&launch, &dir, &vars)
        .unwrap()
        .status()
        .unwrap()
        .success());
    let written = std::fs::read_to_string(dir.join("after.txt")).unwrap();
    assert_eq!(written.trim(), "My Pack my-pack 1");
    let _ = std::fs::remove_dir_all(&dir);
}

/// #772: Play's account reaches the game however it starts: straight, through a wrapper or a
/// pre-launch command, from the Unix holder's copy of the game's words, and from Windows'
/// runtime slot. An offline Play tells the game why.
#[test]
fn the_account_session_reaches_the_game_through_every_launch() {
    let source = std::env::temp_dir().join("sunrise-session-launch");
    let launches = [
        Launch::default(),
        Launch {
            wrapper: "nice -n 5".into(),
            ..Launch::default()
        },
        Launch {
            pre_launch: "echo ready".into(),
            ..Launch::default()
        },
    ];
    for launch in launches {
        let mut cmd = Command::new(source.join(crate::backend::CLIENT));
        cmd.current_dir(&source).env("SUNRISE_HOME", &source);
        crate::run_leases::mark(&mut cmd, &source);
        let session = crate::account::GameSession {
            file: Some("session-1.json".into()),
            ..Default::default()
        };
        assert_eq!(session.apply(&mut cmd), None);
        let cmd = wrap(cmd, &launch);
        let line = |words: Vec<OsString>| {
            words
                .iter()
                .map(|w| w.to_string_lossy())
                .collect::<Vec<_>>()
                .join(" ")
        };
        let env = |key: &str| {
            cmd.get_envs()
                .find(|(k, _)| *k == key)
                .and_then(|(_, v)| v)
                .map(OsStr::to_owned)
        };
        assert!(
            line(cmd.get_args().map(OsStr::to_owned).collect())
                .contains("--session-file session-1.json"),
            "{launch:?}"
        );
        let game = env(crate::run_leases::GAME).unwrap();
        let game: Vec<OsString> = serde_json::from_str(game.to_str().unwrap()).unwrap();
        assert!(line(game).contains("--session-file session-1.json"));
        assert_eq!(env("SUNRISE_OFFLINE_REASON"), None);
        #[cfg(windows)]
        assert!(env("SUNRISE_WINDOWS_RUNTIME_SLOT").is_some());
    }
    let mut cmd = Command::new(source.join(crate::backend::CLIENT));
    let offline = crate::account::GameSession {
        offline: Some("No answer from Microsoft".into()),
        ..Default::default()
    };
    assert_eq!(
        offline.apply(&mut cmd).as_deref(),
        Some("No answer from Microsoft")
    );
    let cmd = wrap(cmd, &Launch::default());
    assert!(!cmd.get_args().any(|a| a == "--session-file"));
    assert!(cmd
        .get_envs()
        .any(|(k, v)| k == "SUNRISE_OFFLINE_REASON"
            && v == Some(OsStr::new("No answer from Microsoft"))));
}
