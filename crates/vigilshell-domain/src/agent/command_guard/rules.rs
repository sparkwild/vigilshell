use super::*;

pub(super) fn read_only(name: &str, args: &[String]) -> AgentResult<()> {
    let (short, long): (&str, &[&str]) = match name {
        "pwd" => ("LP", &[]),
        "ls" => (
            "aAlhndrtSR1F",
            &["all", "almost-all", "directory", "numeric-uid-gid"],
        ),
        "stat" => ("L", &["dereference"]),
        "cat" => ("nbsETvA", &["number", "show-all"]),
        "head" | "tail" => ("qv", &["quiet", "verbose"]),
        "wc" => ("clmwL", &[]),
        "uname" => ("asnrvmpio", &["all"]),
        "id" => (
            "ugGnrz",
            &["user", "group", "groups", "name", "real", "zero"],
        ),
        _ => return Err("read_only_command_denied"),
    };
    let mut operands = 0;
    let mut options = true;
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if options && arg == "--" {
            options = false;
        } else if options && matches!(name, "head" | "tail") && matches!(arg.as_str(), "-n" | "-c")
        {
            i += 1;
            let n = args.get(i).ok_or("invalid_command_arguments")?;
            if n.parse::<u32>().map_or(true, |n| n == 0 || n > 1_048_576) {
                return Err("invalid_command_arguments");
            }
        } else if options && arg.starts_with("--") {
            if !long.contains(&&arg[2..]) {
                return Err("read_only_command_denied");
            }
        } else if options && arg.starts_with('-') && arg != "-" {
            if !arg[1..].chars().all(|c| short.contains(c)) {
                return Err("read_only_command_denied");
            }
        } else {
            if arg == "-" || arg.is_empty() {
                return Err("invalid_command_arguments");
            }
            operands += 1;
        }
        i += 1;
    }
    if matches!(name, "pwd" | "uname") && operands != 0
        || name == "id" && operands > 1
        || matches!(name, "cat" | "head" | "tail" | "wc" | "stat") && operands == 0
    {
        return Err("invalid_command_arguments");
    }
    Ok(())
}

pub(super) fn operate(
    words: &[String],
    redirects: &[(String, String)],
    cwd: &str,
    policy: &ExecutionPolicy,
) -> AgentResult<()> {
    let name = words[0].rsplit('/').next().unwrap_or("");
    let interpreter = matches!(
        name,
        "sh" | "bash" | "dash" | "zsh" | "python" | "node" | "nodejs" | "source" | "."
    ) || name
        .strip_prefix("python")
        .is_some_and(|v| !v.is_empty() && v.chars().all(|c| c.is_ascii_digit() || c == '.'));
    let metadata_query =
        words.len() == 2 && matches!(words[1].as_str(), "--version" | "-V" | "--help" | "-h");
    if interpreter && !metadata_query
        || [".sh", ".bash", ".py", ".js"]
            .iter()
            .any(|s| name.ends_with(s))
    {
        return Err("script_entry_required");
    }
    // 常见委托入口不能藏住被检查的独立命令；不接受另一身份或嵌套 Shell。
    if matches!(
        name,
        "sudo"
            | "su"
            | "env"
            | "command"
            | "exec"
            | "nohup"
            | "xargs"
            | "eval"
            | "busybox"
            | "doas"
            | "pkexec"
            | "runuser"
    ) {
        return Err("unsupported_command_wrapper");
    }
    if matches!(
        name,
        "cd" | "pushd"
            | "popd"
            | "export"
            | "unset"
            | "alias"
            | "unalias"
            | "set"
            | "trap"
            | "read"
            | "umask"
    ) {
        return Err("unsupported_shell_state_change");
    }
    let args = &words[1..];
    for (_, path) in redirects {
        target(path, cwd, false, policy)?;
    }
    match name {
        "rm" | "rmdir" => {
            let mut paths = Vec::new();
            let mut recursive = false;
            let mut options = true;
            for arg in args {
                if options && arg == "--" {
                    options = false;
                    continue;
                }
                if options && arg.starts_with('-') {
                    if (name == "rm"
                        && matches!(
                            arg.as_str(),
                            "--recursive" | "--force" | "--dir" | "--verbose"
                        ))
                        || (name == "rmdir" && matches!(arg.as_str(), "--parents" | "--verbose"))
                    {
                        recursive |= matches!(arg.as_str(), "--recursive" | "--parents");
                    } else if !arg.starts_with("--")
                        && arg.len() > 1
                        && arg[1..]
                            .chars()
                            .all(|c| (if name == "rm" { "rfidvRI" } else { "pv" }).contains(c))
                    {
                        recursive |= arg.contains(['r', 'R', 'p']);
                    } else {
                        return Err("invalid_command_arguments");
                    }
                } else {
                    paths.push(arg);
                }
            }
            if paths.is_empty() {
                return Err("invalid_command_arguments");
            }
            for path in paths {
                target(path, cwd, recursive, policy)?;
            }
        }
        "chmod" | "chown" | "chgrp" => {
            let mut operands = Vec::new();
            let mut recursive = false;
            let mut options = true;
            for arg in args {
                if options && arg == "--" {
                    options = false;
                    continue;
                }
                if options && arg.starts_with('-') {
                    if arg == "--recursive" {
                        recursive = true;
                    } else if !arg.starts_with("--")
                        && arg.len() > 1
                        && arg[1..].chars().all(|c| "RvcfhLPH".contains(c))
                    {
                        recursive |= arg.contains('R');
                    } else {
                        return Err("invalid_command_arguments");
                    }
                } else {
                    operands.push(arg);
                }
            }
            if operands.len() < 2 {
                return Err("invalid_command_arguments");
            }
            if name == "chmod" && !valid_mode(operands[0]) {
                return Err("invalid_command_arguments");
            }
            for path in &operands[1..] {
                target(path, cwd, recursive, policy)?;
            }
        }
        "cp" | "mv" | "install" | "ln" => {
            let mut paths = Vec::new();
            let mut target_directory = None;
            let mut options = true;
            let mut i = 0;
            let recursive = args
                .iter()
                .any(|a| a == "--recursive" || a.starts_with('-') && a.contains(['r', 'R', 'a']));
            while i < args.len() {
                let arg = &args[i];
                if options && arg == "--" {
                    options = false;
                } else if options && matches!(arg.as_str(), "-t" | "--target-directory") {
                    i += 1;
                    target_directory =
                        Some(args.get(i).ok_or("invalid_command_arguments")?.as_str());
                } else if options && arg.starts_with("--target-directory=") {
                    target_directory = arg.split_once('=').map(|(_, v)| v);
                } else if options && arg.starts_with('-') {
                    if arg.starts_with("--")
                        || arg.len() == 1
                        || !arg[1..].chars().all(|c| "aRrfivnpsTLH".contains(c))
                    {
                        return Err("invalid_command_arguments");
                    }
                } else {
                    paths.push(arg.as_str());
                }
                i += 1;
            }
            if paths.is_empty() || target_directory.is_none() && paths.len() < 2 {
                return Err("invalid_command_arguments");
            }
            if name == "mv" {
                for path in &paths {
                    target(path, cwd, recursive, policy)?;
                }
            }
            target(
                target_directory.unwrap_or(paths[paths.len() - 1]),
                cwd,
                recursive,
                policy,
            )?;
        }
        "tee" | "touch" | "truncate" => {
            let paths: Vec<_> = args.iter().filter(|a| !a.starts_with('-')).collect();
            if paths.is_empty() {
                return Err("invalid_command_arguments");
            }
            for path in paths {
                target(path, cwd, false, policy)?;
            }
            if args
                .iter()
                .any(|a| a.starts_with("--") && a != "--" && a != "--append")
            {
                return Err("invalid_command_arguments");
            }
        }
        "dd" => {
            let outputs: Vec<_> = args.iter().filter_map(|a| a.strip_prefix("of=")).collect();
            if outputs.len() != 1 {
                return Err("invalid_command_arguments");
            }
            target(outputs[0], cwd, true, policy)?;
        }
        "find"
            if args.iter().any(|a| {
                matches!(
                    a.as_str(),
                    "-delete" | "-exec" | "-execdir" | "-ok" | "-okdir"
                )
            }) =>
        {
            return Err("unsupported_destructive_command");
        }
        "sed"
            if args
                .iter()
                .any(|a| a.starts_with("-i") || a.starts_with("--in-place")) =>
        {
            return Err("unsupported_destructive_command");
        }
        "mkfs" | "wipefs" | "fdisk" | "parted" | "shred" | "userdel" | "usermod" | "passwd"
        | "visudo"
            if policy.protect_system =>
        {
            return Err("dangerous_system_command");
        }
        _ if name.starts_with("mkfs.") && policy.protect_system => {
            return Err("dangerous_system_command");
        }
        _ => {}
    }
    Ok(())
}
fn target(path: &str, cwd: &str, recursive: bool, policy: &ExecutionPolicy) -> AgentResult<()> {
    if path.is_empty() || path == "-" || path.contains(['*', '?', '$', '`', '{', '}', '[', ']']) {
        return Err("ambiguous_command_target");
    }
    let normalized = normalize_path(cwd, path)?;
    if normalized == "/" || recursive && matches!(path, "." | "..") {
        return Err("dangerous_target");
    }
    if policy
        .protected_paths
        .iter()
        .any(|p| beneath(&p.path, &normalized) || recursive && beneath(&normalized, &p.path))
    {
        return Err("protected_target");
    }
    if policy.protect_system {
        const ROOTS: &[&str] = &[
            "/etc",
            "/boot",
            "/dev",
            "/proc",
            "/sys",
            "/bin",
            "/sbin",
            "/lib",
            "/lib64",
            "/usr",
            "/root/.ssh",
            "/var/lib",
            "/var/log",
            "/var/run",
            "/run",
        ];
        if ROOTS
            .iter()
            .any(|p| beneath(p, &normalized) || recursive && beneath(&normalized, p))
            || ["/home", "/root", "/var", "/srv", "/opt", "/tmp"].contains(&normalized.as_str())
            || recursive && normalized.starts_with("/home/") && normalized.matches('/').count() == 2
            || normalized.split('/').any(|p| p == ".ssh")
        {
            return Err("dangerous_system_target");
        }
    }
    Ok(())
}

fn valid_mode(mode: &str) -> bool {
    if !mode.is_empty() && mode.len() <= 4 && mode.chars().all(|c| ('0'..='7').contains(&c)) {
        return true;
    }
    mode.split(',').all(|clause| {
        let Some(index) = clause.find(['+', '-', '=']) else {
            return false;
        };
        clause[..index].chars().all(|c| "ugoa".contains(c))
            && clause[index + 1..].chars().all(|c| "rwxXstugo".contains(c))
    })
}
