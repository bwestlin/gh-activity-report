# Contributing

## Pull requests

Pull request titles must follow
[Conventional Commits](https://www.conventionalcommits.org/):

```text
<type>[optional scope][!]: <description>
```

Allowed types are `feat`, `fix`, `docs`, `test`, `ci`, `refactor`, `perf`,
`chore`, and `revert`.

Examples:

```text
feat(activity): group pushes by branch
fix(github): preserve cross-repository issue links
docs: clarify token permissions
feat(output)!: rename a JSON report field
```

Pull requests must also include a short description. The `PR policy` workflow
checks both requirements.

Pull requests should be squash-merged with the validated pull request title as
the squash commit subject. This keeps the commit on `main` compatible with the
generated changelog.

## Local checks

Run the project checks before opening a pull request:

```sh
just check-all
```
