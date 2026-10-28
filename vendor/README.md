# Vendored dependencies

## fynix

- Upstream: https://github.com/voxell-tech/fynix
- Base commit: `6aea143ad9fd406e87043e2f9a9b373064378107`
- Licence: `MIT OR Apache-2.0`

`vendor/fynix` started as a copy of that commit, made with `git archive`. It holds no `.git` directory, no submodule, and no subtree.

rubrix PRs have changed the copy since then. The main changes are these:

- The `rich` element, in `crates/bevy_fynix/src/views/rich.rs`, with `rich`, `rich_layout`, and `rich_point`. It hands out the text layout from `parley` 0.11.1, the version that `bevy_text` 0.20 uses, so `bevy_fynix` depends on that `parley` directly.
- The virtual list, in `crates/bevy_fynix/src/views/virtual_list.rs`, with `virtual_list` and `VirtualList`. It builds only the rows near the visible area.
- Changes to scroll easing in `crates/bevy_fynix/src/scroll.rs`, made in the virtual list and code view PRs.

Changes to fynix are rubrix PRs that edit the files here. Nothing is pushed to voxell-tech/fynix or to any fork of it.

To list the rubrix commits that changed the copy:

```sh
git log --oneline -- vendor/fynix
```

To compare the copy with the base commit:

```sh
git clone https://github.com/voxell-tech/fynix.git /path/to/scratch
git -C /path/to/scratch checkout 6aea143ad9fd406e87043e2f9a9b373064378107
diff -rq --exclude=.git --exclude=target /path/to/scratch vendor/fynix
```

The copy is not identical to the base commit, so the diff is not empty. It lists each file that rubrix changed or added. Drop `-q` to see the changes themselves.
