# Runbook: a leaked source credential

Do these steps in order when the private key of the GitHub App, a host
token or a runner credential can have leaked. The key of the App can also
edit runner groups.

1. Rotate the key of the App in the settings of the App. Put the new key
   on each `mint` host. Delete the old key.
2. List the runners and the scale sets of each organization. Delete each
   one that `podrunner status` of no host lists.
3. Make sure that the App is installed on named repositories only, not on
   all repositories of the organization.
4. Make sure that each runner group of a trusted set refuses public
   repositories.
5. Alert on each registration whose runner name is not in the list that
   the hosts write (`podrunner-HOST-u-N`).
6. Revoke the host token or the key of each host that can have leaked, at
   `mint` (T-031).
7. Write what happened as an entry `T-NNN` of `TODO/` with the times, and
   add the check that would have found it.
