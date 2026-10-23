# Vendored dependencies

## fynix

- Upstream: https://github.com/voxell-tech/fynix
- Commit: `6aea143ad9fd406e87043e2f9a9b373064378107`
- Licence: `MIT OR Apache-2.0`

`vendor/fynix` is a plain copy of that commit, made with `git archive`. It holds no `.git` directory, no submodule, and no subtree.

Changes to fynix are rubrix PRs that edit the files here. Nothing is pushed to voxell-tech/fynix.

To check the copy against upstream:

```sh
git clone https://github.com/voxell-tech/fynix.git /path/to/scratch
git -C /path/to/scratch checkout 6aea143ad9fd406e87043e2f9a9b373064378107
diff -r --exclude=.git /path/to/scratch vendor/fynix
```

The diff lists every file that rubrix changed since the copy.
