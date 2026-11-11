# Security

podrunner runs code of CI jobs, which can be hostile. The threat model,
the accepted residuals and the runbook for a leaked credential come with
T-019.

## Report a vulnerability

Open a private security advisory on
<https://github.com/PodUtils/podrunner/security/advisories/new>. Do not
put a credential, a token or a log with one in a public issue.

## Rules of the code

* No credential in argv, in the environment of a unit, in a log, in the
  journal or in a report. Credentials reach a unit as files of mode 0600.
* Each binary is static and is built by CI from a tagged commit.
