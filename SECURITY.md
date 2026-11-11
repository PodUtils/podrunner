# Security

podrunner runs code of CI jobs, which can be hostile.
`docs/security/threat-model.md` gives the actors and the controls,
`docs/security/residuals.md` the risks that the design accepts, and
`docs/security/leak-runbook.md` the steps after a leaked credential.

## Report a vulnerability

Open a private security advisory on
<https://github.com/PodUtils/podrunner/security/advisories/new>. Do not
put a credential, a token or a log with one in a public issue.

## Rules of the code

* No credential in argv, in the environment of a unit, in a log, in the
  journal or in a report. Credentials reach a unit as files of mode 0600.
* Each binary is static and is built by CI from a tagged commit.
