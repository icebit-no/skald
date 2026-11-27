# Working method

How Skald is built: a lead session runs the work, workers build one pull request each, and Thomas decides. What
to build, in which order, is the [plan](plan.md).

## Roles

- **Thomas** owns the product. He takes every decision listed under
  [Waiting on Thomas](plan.md#waiting-on-thomas) and merges what the lead may not.
- **The lead** is one Claude session Thomas starts, usually in the evening, on the newest Opus at `xhigh`
  (`claude --model claude-opus-5-5 --effort xhigh`). It reads the plan, writes a brief per pull request,
  starts and watches the workers, reviews, keeps the plan true and reports to Thomas. It asks Thomas whenever
  something needs a decision, and otherwise keeps going.
- **A code worker** is a fresh Claude session for one pull request, on the newest Opus at `xhigh`, in a Herdr
  tab and a git worktree of its own. It builds what its brief says, opens the pull request, and stops.
- **A graphics worker** is the same, on Fable (`claude --model claude-fable-5-2`): icons, illustrations, the
  hero of the site, and the site's visual work where the lead finds it fits.
- A worker never decides what belongs to Thomas; it writes the question in its pull request and tells the
  lead.

## Starting a worker

Workers appear in the Herdr window as sub-tabs under the lead, indented beneath it. That happens when the lead
starts them with `hc run` from its own terminal, which makes the worker a child of the lead:

```sh
git worktree add ../skald-worktrees/<slug> -b thomastvedt/<slug> main
hc run --bg --name skald-<slug> --cwd ../skald-worktrees/<slug> \
  claude --model claude-opus-5-5 --effort xhigh "<brief>"
```

- Start workers with `hc run`, never with `herdr-app do tab.new`: a tab made that way has no parent and shows
  up as a tab of its own, not under the lead.
- Put the brief in the `run` call; a message sent to an agent that is still starting can be lost. Check that
  `first_message.sent` is true in the reply.
- A child ends when its parent ends. Keep the lead running for the whole run, and close each worker after its
  pull request is merged (`hc kill skald-<slug>; hc rm skald-<slug>`).
- Names are unique: `skald-<slug>`. The branch `thomastvedt/<slug>` is created from local `main` and tracks
  nothing.
- At most two code workers at a time, on separate areas, so they do not get in each other's way.
- The brief names the plan item, the sections of design.md and research.md it needs, the files it touches,
  what done means (it builds, the tests pass, the plan item is ticked, the pull request is open), and what the
  worker must not decide. It points at files rather than retelling them.
- Workers report with `hc send <lead> "…"` when done or stuck; the lead waits with `hc wait --children any`
  rather than polling.

## Pull requests

- One pull request per plan item, titled `Area type: Description` (for example `Engine feature: Add
  transcribe.cpp with NB-Whisper`), with a `## Related` section first.
- No force-push and no rebase by workers. A branch that falls behind merges `origin/main`.
- The Mac is shared with Thomas and other projects: a worker runs the tests its change touches and leaves the
  full suite to CI.
