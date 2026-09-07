---
id: 50
title: Deploy from CI, and from nowhere else
type: chore
status: done
milestone: v1.0
depends_on:
- 49
created: 2026-09-07
updated: 2026-09-07
priority: p0
effort: s
area: packaging
---

## Problem

A site that is deployed by hand is deployed from whatever was in someone's
working tree, which is how a preview build, a debug banner, or an unfinished
page ends up in production. There is also no answer to "what is live?" other
than looking.

## Proposal

GitHub Pages, published by GitHub Actions from `main`, and by nothing else.

- **One workflow, `pages.yml`**, triggered on push to `main` and manually.
  Build with the same `site build` CI already runs, upload the artifact, deploy
  with `actions/deploy-pages`.
- **Deploy only if CI is green.** A broken link should not be published because
  the deploy job happened to run first.
- **`concurrency: pages` with cancel-in-progress**, so two merges in a minute
  do not race and leave the older one live.
- **No secrets.** OIDC permissions (`pages: write`, `id-token: write`) scoped
  to that job alone.
- **The base URL is a build input.** `site build --base-url` so the same
  generator produces a site for `oddurs.github.io/trafford/`, for a custom
  domain, and for `file://` in development, without a second code path. Getting
  this wrong is the standard Pages failure: every asset 404s under a project
  subpath because the paths were written as if the site were at a domain root.
- **A real 404 page**, styled like the site, since Pages will serve `404.html`.

## Acceptance criteria

- [ ] A push to `main` publishes within a few minutes with no human step
- [ ] The published site is what the same commit builds locally
- [ ] Assets, links and anchors all resolve under the project subpath
- [ ] A failed build publishes nothing and leaves the previous site up
- [ ] A pull request never publishes
- [ ] `404.html` is styled and links back into the docs

## Notes

Whether there is a custom domain is a separate decision and changes only
`--base-url` and a `CNAME` file. Do not design for it now, but do not write
absolute URLs that would have to be found and changed when it happens.

A preview deployment per pull request is genuinely useful and genuinely a
security decision — a Pages preview means running a build from a fork's branch.
Leave it out until someone other than the author is opening pull requests.

## What changed on the way

**`--base-url` is not needed and does not exist.** Every href the build writes
is relative to the page holding it, so the same tree is correct at a domain
root, under `/trafford/`, and over `file://`. What the deploy passes is
`--site-url`, which only fills in the canonical tags and the sitemap. The
classic Pages failure the criterion was written against — every asset 404s
under a project subpath — cannot happen, because there is no absolute path to
get wrong.

**`workflow_run`, not a duplicated gate.** "Deploy only if CI is green" is a
trigger rather than a set of repeated steps: `pages.yml` fires on `ci`
completing successfully on `main`, and checks out `head_sha` rather than the
branch tip, which is not always the commit that passed.
