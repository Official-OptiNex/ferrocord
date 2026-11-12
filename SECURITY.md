# Security policy

ferrocord runs code inside a program that holds your Discord session, so security issues matter.

## Reporting a vulnerability

**Please don't open a public issue.** Report it privately through
[GitHub's private vulnerability reporting](https://github.com/Official-OptiNex/ferrocord/security/advisories/new).

Please include:

- What the issue is and where it is
- Steps to reproduce
- The impact you expect (for example, a plugin escaping the sandbox)

You should get a reply within a few days.

## Scope

In scope:

- Plugins escaping the WASM sandbox or reading host memory
- Ways to load the loader into processes it shouldn't touch
- Anything that could expose a user's token or account

Out of scope: Discord's own client and services. Report those to Discord.

## Plugin safety

Plugins run in a sandbox with no file or network access, but they can still see the events ferrocord sends
them. Only install plugins from sources you trust.
