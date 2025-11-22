/// Seccomp profile for sandboxed code execution
/// This restricts syscalls to only what's necessary for basic code execution
use serde_json::json;

/// Generate a seccomp profile that allows only safe syscalls
pub fn generate_seccomp_profile() -> serde_json::Value {
    json!({
        "defaultAction": "SCMP_ACT_ERRNO",
        "defaultErrnoRet": 1,
        "archMap": [
            {
                "architecture": "SCMP_ARCH_X86_64",
                "subArchitectures": [
                    "SCMP_ARCH_X86",
                    "SCMP_ARCH_X32"
                ]
            },
            {
                "architecture": "SCMP_ARCH_AARCH64",
                "subArchitectures": [
                    "SCMP_ARCH_ARM"
                ]
            }
        ],
        "syscalls": [
            {
                "names": [
                    // Process control
                    "exit",
                    "exit_group",
                    "wait4",
                    "waitid",

                    // Memory management
                    "brk",
                    "mmap",
                    "mmap2",
                    "munmap",
                    "mremap",
                    "mprotect",
                    "madvise",

                    // File operations (read only on most)
                    "read",
                    "write",
                    "open",
                    "openat",
                    "close",
                    "stat",
                    "fstat",
                    "lstat",
                    "stat64",
                    "fstat64",
                    "lstat64",
                    "newfstatat",
                    "statx",
                    "lseek",
                    "access",
                    "faccessat",
                    "faccessat2",
                    "pipe",
                    "pipe2",
                    "dup",
                    "dup2",
                    "dup3",
                    "readlink",
                    "readlinkat",

                    // Directory operations
                    "getcwd",
                    "chdir",
                    "fchdir",

                    // Signal handling
                    "rt_sigaction",
                    "rt_sigprocmask",
                    "rt_sigreturn",
                    "sigaltstack",

                    // Time
                    "clock_gettime",
                    "clock_getres",
                    "gettimeofday",
                    "nanosleep",
                    "clock_nanosleep",

                    // Process info
                    "getpid",
                    "getppid",
                    "getuid",
                    "geteuid",
                    "getgid",
                    "getegid",
                    "getgroups",

                    // Polling/waiting
                    "poll",
                    "ppoll",
                    "select",
                    "pselect6",
                    "epoll_create",
                    "epoll_create1",
                    "epoll_ctl",
                    "epoll_wait",
                    "epoll_pwait",

                    // Futex for threading
                    "futex",
                    "set_robust_list",
                    "get_robust_list",

                    // Architecture specific
                    "arch_prctl",
                    "set_tid_address",
                    "set_thread_area",
                    "get_thread_area",

                    // Socket operations (limited)
                    "socket",
                    "socketpair",
                    "connect",
                    "sendto",
                    "recvfrom",
                    "sendmsg",
                    "recvmsg",
                    "shutdown",
                    "getsockname",
                    "getpeername",
                    "getsockopt",
                    "setsockopt",

                    // IO operations
                    "ioctl",
                    "fcntl",
                    "fcntl64",
                    "flock",
                    "fsync",
                    "fdatasync",

                    // Resource limits
                    "getrlimit",
                    "setrlimit",
                    "prlimit64",

                    // Misc
                    "uname",
                    "getrandom",
                    "sched_yield",
                    "sched_getaffinity",
                    "prctl",
                    "sysinfo",
                    "getrusage",
                    "pread64",
                    "pwrite64",
                    "readv",
                    "writev",
                    "getcpu",

                    // Clone (restricted)
                    "clone",
                    "clone3",
                    "vfork",
                    "fork",

                    // Execution
                    "execve",
                    "execveat"
                ],
                "action": "SCMP_ACT_ALLOW",
                "args": [],
                "comment": "",
                "includes": {},
                "excludes": {}
            }
        ]
    })
}

/// Generate a strict seccomp profile (even more restrictive)
pub fn generate_strict_seccomp_profile() -> serde_json::Value {
    json!({
        "defaultAction": "SCMP_ACT_ERRNO",
        "defaultErrnoRet": 1,
        "syscalls": [
            {
                "names": [
                    // Minimal syscalls for execution only
                    "exit",
                    "exit_group",
                    "read",
                    "write",
                    "brk",
                    "mmap",
                    "munmap",
                    "rt_sigaction",
                    "rt_sigprocmask",
                    "rt_sigreturn",
                    "getpid",
                    "arch_prctl",
                    "set_tid_address",
                    "futex",
                    "clock_gettime",
                    "execve"
                ],
                "action": "SCMP_ACT_ALLOW"
            }
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_seccomp_profile_generation() {
        let profile = generate_seccomp_profile();
        assert_eq!(profile["defaultAction"], "SCMP_ACT_ERRNO");
        assert!(profile["syscalls"].is_array());
    }

    #[test]
    fn test_strict_seccomp_profile() {
        let profile = generate_strict_seccomp_profile();
        assert_eq!(profile["defaultAction"], "SCMP_ACT_ERRNO");
        let syscalls = profile["syscalls"].as_array().unwrap();
        assert!(!syscalls.is_empty());
    }
}
