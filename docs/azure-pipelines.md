# Azure Pipelines YAML in the local mode (T-148)

The result of a reading. No code of podrunner reads Azure Pipelines YAML.

## What runner.server does

runner.server (the study of the owner, `--event azpipelines`) runs a
subset of Azure Pipelines YAML:

* The template expansion: `extends`, `template:` files of steps, jobs,
  stages and variables, `parameters` with types, `${{ each }}`,
  `${{ if }}`, `${{ elseif }}`, `${{ else }}` and the functions of Azure
  Pipelines. This part is about 4,000 lines of C# in
  `src/Sdk/AzurePipelines` (28 files), with its own schema
  (`azurepiplines.json`).
* The steps `script`, `bash`, `pwsh`, `powershell`, `checkout`, `task` and
  `template`. A `script`, `bash` or `pwsh` step becomes a *task* step
  (`TaskStep`, with the input `script`), not an action step.
* The job message is `AgentJobRequestMessage` for the **Azure Pipelines
  agent** (`microsoft/azure-pipelines-agent`), with the variables of the
  agent (`System.*`, `Build.*`, `agent.*`). The official GitHub runner
  (`actions/runner`) does not run task steps.

## What podrunner would need

1. A second payload: the Azure Pipelines agent (.NET), in a second runner
   image, with its own versions and advisories.
2. A second fake broker: the routes of the agent (the pool, the session,
   the messages, the timeline, the logs), which differ from the routes of
   the GitHub runner.
3. A store of tasks: each `task: NAME@MAJOR`, and each `script` step
   (the tasks `CmdLine`, `Bash`, `PowerShell`), is a zip of the task that
   the agent downloads. The tasks come from Azure DevOps or from
   `microsoft/azure-pipelines-tasks`, with a pin, as the actions of T-118.
4. The template expansion, in Rust: a crate of the size of
   `pod-actions-workflow`, generic, so a request to podcrates.
5. A second set of contexts and of variables, and a second parity suite.

The cost is the cost of a second local mode. It shares the units, the
egress module, the hooks and the drive loop with the first one.

## Recommendation

Do not build it now. The local mode of GitHub workflows has open entries
(parallel jobs, the unit parts of T-138, a debugger). No user of
podrunner asked for Azure Pipelines. If the owner wants it, the first step
is a measure: the Azure Pipelines agent in a podvm unit against a fake
broker with one canned message, as T-026 did for the GitHub runner. The
question is Q6 of `TODO/PROGRESS.md`.
