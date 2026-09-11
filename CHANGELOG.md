## What's Changed in 0.2.1
* feat: improve config errors and CLI diagnostics
* feat(shell): generate Omarchy configuration from Rust
* docs: update README
* fix(renderer): improve failure feedback and keyboard interaction
* docs: update README
* chore: exclude generated changelog from formatting

**Full Changelog**: https://github.com/roushou/omega/compare/v0.2.0...v0.2.1

## What's Changed in 0.2.0
* docs: fix rustdoc links and schema formatting
* docs: update README files
* refactor!: organize public API by domain by @roushou
* feat!: add typed commands and declarative UI components by @roushou
* feat: add interactive plugins and improve widget UX by @roushou
* refactor!: harden runtime and adopt async commands by @roushou
* fix(renderer): reconnect a host that loaded before the daemon by @roushou
* docs: cut history and rejected alternatives from comments by @roushou
* refactor(proto): reset the wire version to 1 by @roushou
* fix(cli): scaffold a widget that does not redden a filling battery by @roushou
* docs(renderer): state the constraint, not the story by @roushou
* feat(renderer): give a node the room it is in by @roushou
* feat(brokers): report how hot the machine is by @roushou
* feat(brokers): report what is moving over the network by @roushou
* feat(brokers): report the power profile the machine is actually in by @roushou
* refactor(sdk): name a field for what it is, and composite what spans topics by @roushou
* refactor(sdk): scope the root into state, ui, effect and config by @roushou
* feat(sdk): close the taxonomies the view and state layers left open by @roushou
* feat(renderer): let text name a size on the shell's type scale by @roushou
* fix(renderer): size a panel to what it draws by @roushou
* feat(renderer): draw every node through the shell's design tokens by @roushou
* fix(renderer): let a press on the slot open the panel by @roushou
* feat(daemon): fire the schedules the document declares by @roushou
* ci: stop a third-party apt repo from failing the qml lint by @roushou
* refactor: make the manifest a schema message and split out omega-host by @roushou
* refactor(proto): rename the Topic address enum to Address by @roushou
* refactor(sdk): rename the Topic trait to UnitState by @roushou
* feat(renderer): generate the shell's prop readers from the vocabulary by @roushou
* feat(daemon): let an observer choose what it is sent by @roushou
* fix(daemon): let the hub survive a poisoned lock everywhere by @roushou
* fix(daemon): name the program a unit could not run by @roushou
* fix(daemon): a machine with no build is not a failed one by @roushou
* docs: the brokered topics and actions are no longer unbuilt by @roushou
* fix(brokers): stop a broker with no actions from spinning by @roushou
* refactor: prefer if/else over match on bool by @roushou
* ci: lint the qml on whatever qt the runner has by @roushou
* refactor(brokers): let the driver hold the connection rules by @roushou
* feat(sdk): give every topic a handle by @roushou
* feat(brokers): report vpn tunnels and disk usage by @roushou
* feat(brokers): report peripheral batteries and keyboard layout by @roushou
* feat(ui): add group, grid and image nodes by @roushou
* feat(ui): add the graph node by @roushou
* feat(ui): add header, separator, spacer, and shared control state by @roushou
* chore: drop unused dependencies by @roushou
* ci: lint the qml the renderer ships by @roushou
* feat(brokers): launch apps, capture screens, and report idle by @roushou
* feat(brokers): report cpu, memory and load from procfs by @roushou
* feat(brokers): report bluetooth devices from bluez by @roushou
* feat(brokers): report and control players over mpris by @roushou
* feat(brokers): report workspaces and focus from hyprland by @roushou
* feat(brokers): read and set the volume through pactl by @roushou
* feat(brokers): report mains power from upower by @roushou
* feat(brokers): send notifications to the desktop by @roushou
* feat(brokers): dispatch window and workspace actions to hyprland by @roushou
* feat: publish wifi scan results as a topic by @roushou
* feat: publish the wall clock as a topic by @roushou
* feat(brokers): serve the session actions from logind by @roushou
* feat(sdk): let a unit publish a list by @roushou
* test(sdk): rebuild the wifi panel as an omega unit by @roushou
* feat(ui): add field and list nodes by @roushou
* feat(brokers): read the displays from hyprland by @roushou
* feat(brokers): read the network from networkmanager by @roushou
* feat(brokers): read the battery from upower by @roushou
* feat: brokers, event bindings, and panel surfaces by @roushou
* docs: add design doc by @roushou
* fix: start the daemon on a machine with nothing built yet by @roushou
* chore: complete crate metadata for publishing by @roushou
* feat: publish the SDK as omega-rs by @roushou
* ci: check unused dependencies and supply chain by @roushou
* feat: add omega clean by @roushou
* fix: build into the config's own target dir by @roushou
* docs: cut the prose to the constraints by @roushou
* docs: state what omega needs to run by @roushou
* refactor: fold eight crates into six by @roushou
* feat: draw icons as glyphs instead of their own names by @roushou
* Ready, Go! by @roushou

### New Contributors
* @roushou made their first contribution

<!-- generated by git-cliff -->
