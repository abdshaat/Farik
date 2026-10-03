---
name: reviewing-dependencies
description: Use when a contract or a diff adds a library or upgrades one, so that you decide whether the project should depend on it.
---

# Reviewing dependencies

A library is code the team did not write and will have to keep. Decide before it is taken on.

## 1. Why this one

- Say what it does for the project and why code the project already has does not.
- Prefer the smaller choice. A library for ten lines of code is a cost with no gain.

## 2. Licence

Read the package's own licence file where the project keeps a copy of it (a vendored folder, when
there is one), or, in a document session, the registry's page with `WebFetch`. In a review session you run no
commands and have no `WebFetch`: judge from what the diff and the project show, and say what you
could not check.

## 3. Maintenance

Is it still updated? Is one person the only maintainer? Does it pull in many other packages? Name
what you found and where.

## 4. Known flaws

When OSV is connected, ask it (see `using-architecture-sources`) for the exact version the lock
file pins. A flaw with a fixed version means: take the fixed version, or write why not.

## 5. The version

Name the version the lock file pins, or should. Prefer an exact one.

## 6. Write it down

In a document task, record the choice with `farik_write_decision`: the library, the version, why,
and what it rules out. In a review session, a finding goes to the `review` criterion it bears on,
with the file and line, through `farik_record_criterion_result`; record a decision there only when
the contract asks for one.
