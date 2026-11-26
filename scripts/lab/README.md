# The labs of podrunner

Each script brings up one host class (a profile of COORD-061, GitHub
#180) on a disposable host, checks that the class holds, and runs a
command in it:

```sh
sh scripts/lab/PROFILE.sh -- COMMAND ARGS...   # inherited-fds.sh: bash
```

The scripts are the first draft of the profiles of `pod-lab` (podcrates)
and `podvm lab run` (podvm); podrunner drops each one for theirs when it
pins them (T-244). A script that needs root says so, and runs only on a
disposable host (a GitHub runner).

| Script | The class | Root |
| --- | --- | --- |
| `small-tmp.sh` | `TMPDIR` on a tmpfs of 1 GiB (`LAB_TMP_SIZE`) | yes (the mount) |
| `userns-off.sh` | no user namespace (`user.max_user_namespaces=0`) | yes (the sysctl) |
| `userns-on.sh` | a user with no root makes a user namespace with mounts (AppArmor's restriction off), so podvm takes its namespace rung | yes (the sysctl) |
| `nonroot.sh` | a user with no root and no capability | yes (the user) |
| `proxy-only.sh` | the user reaches the network only through an HTTPS proxy on the loopback that intercepts TLS (mitmproxy), with the proxy and a CA bundle in its environment; before `nonroot.sh` | yes (the user, the proxy, iptables) |
| `fsize.sh` | the soft and the hard limit of file size are 1 GiB (`LAB_FSIZE` bytes; `prlimit`); before `nonroot.sh` | no |
| `inherited-fds.sh` | descriptors 3 to N open in the command (`LAB_FDS`, default 15); bash | no |

Each script checks its class first and fails with the part that does not
hold. `.github/workflows/user-gate.yml` (T-243) runs them as the
lanes of the user-path gate, one lane for each class. Each one is 0BSD, as the repository.
