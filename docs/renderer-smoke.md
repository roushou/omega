# Renderer desktop smoke test

Run this on an unlocked Omarchy desktop after changing panel hosting, installation,
or attachment handling. It complements the headless QML tests: those tests cannot
verify compositor input routing or which components a running shell has cached.
The checks below require manual clicks; CLI status alone does not validate visuals.

## Update and confirm the running build

1. Build the CLI and install it at the service's executable path. Restart the
   Omega daemon before installing a renderer that uses a changed protocol.
2. Run `omega shell install`. Expect a shell restart followed by a verified
   running renderer message. A restart failure must fail the command; installed
   files do not count as successful activation.
3. Run `omega shell status` and `omega status --json`. Every active placement
   should report a nonempty build fingerprint matching the CLI. Linked checkouts
   are explicitly unverified and are not suitable for this identity check.

To exercise stale-code detection, use a CLI containing changed renderer assets
and run `omega shell install --no-restart`. Status must not claim that matching
installed files prove the running renderer is current. If the host has already
reloaded the new QML, it may legitimately report the new fingerprint. Finish with
`omega shell install` and confirm activation.

## Panel interaction

Use two configured Omega widgets with panels, such as audio and power.

1. Click the first indicator once. Leave the pointer still for five seconds.
   Its panel must stay open, including while readings update.
2. Interact with a control whose effect you can safely undo, then restore its
   value. The control must receive the interaction and retain keyboard focus.
3. Click the second indicator. Its panel must replace the first in one click.
4. Click outside the panel. It must close. Reopen it and press Escape; it must close.
5. Rapidly click an indicator three times. The final intent must win without
   repeated open/close animation or a permanently pending interaction.
6. With a panel open, restart `omega.service`. The renderer must disconnect,
   clear the old view, and reattach. Open the panel again and repeat a control
   interaction. `omega status` must report healthy plugins and current renderers.

If a check fails, record which input caused it, the delay before closure, and
`omega status --json`. Read the current shell log with
`quickshell log -p "$OMARCHY_PATH/shell" --tail 100 --log-times`.
