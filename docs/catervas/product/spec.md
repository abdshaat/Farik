# Catervas

Catervas runs a small team of AI agents on one project for one person. The team has a Product Manager, a Scrum Master, an Architect, a Developer, a Marketing Specialist and other roles, and it plans, builds, reviews and ships the work together. The problem it solves: a group of agents pointed at a project tends to go wrong in ways that are about how the group is organised, not how clever the model is. Nobody pins down what the task is, the agent that did the work also says it is correct, and afterwards nobody can say why something changed or what it cost. Catervas fixes this with rules written in code, not in a prompt. No work starts without a written contract, nobody approves their own work, every agent has spending and time limits, and everything that happens is recorded. In the project's own words, the governance layer is the product.

## Who it is for

- **The non-technical founder or creator, first.** Someone who knows what they want built but cannot read code. They must be able to set up a team, approve its plans and accept its work from plain-language summaries alone, and never be asked for a file pattern or a file format unless they open an advanced setting.
- **The solo builder.** Technical enough to read a code change, wants the product to keep moving while they are at their day job, watches costs, runs on their own AI account, and cares that unsupervised agents do not wreck the project.
- **The small-team lead.** Runs a company of two to five people and wants an always-on extra team for backlog grooming, routine implementation and release notes. Would pay for hosting if it saves an afternoon a week and the record of what happened is good enough to trust.
- **The small-business owner, later.** Runs a business that is not software, such as flipping cars or reselling goods online, and wants a team that researches, buys through orders the owner approves, keeps the books and markets the business. Served fully after the first launch.
- Large companies' platform teams are deliberately left for later.

## What it must do

1. Let a person build a team of two to seven agents, each with a name, a picture and a role, with at least a Product Manager and a Developer.
2. Point the team at an existing project, or start a new one from a short description.
3. Give every task a written contract, with clear checks for "done", before anyone starts on it, and never let anyone accept their own work: the reviewer is never the author.
4. Check every action an agent takes against the team's rules in code, so an agent can only ask, and the rules decide.
5. Record every action and every cost in a log that is never rewritten, so each expense belongs to a task and an agent.
6. Let the person talk to any agent privately, and read and join the team's group chat.
7. Give each agent its own connected services and its own skills.
8. Run on the person's own computer with their own AI account; their code does not leave the machine.
9. Let a non-technical person set up and run a team in the web app with no terminal, one question per screen, with every decision that is theirs explained in plain words and the code change one click away for whoever reads code.
10. Bound every working session by time, tool use and tries, keep spending limits optional, and bring anything that hits a limit to the person rather than letting it run on.

## Limits

- A team has at most seven agents.
- One person, one team: no shared teams yet.
- A person must be reachable for questions and escalations, even if hours pass before they answer.
- The rules check on each action must take under five milliseconds.
- The app must stay quick with ten thousand recorded events in a project.
- Every screen meets the WCAG 2.2 AA accessibility standard, and the web app works from 360 pixels wide.
- English only in the first release.
- Everything works offline except the AI model and services that need the internet.
- A first day of the suggested team of six running a full sprint is estimated at about twenty-five dollars. This is an estimate of the defaults, not a cap.

## Not now

- The desktop app and the phone apps. The first launch is the web app alone; the desktop app follows once the web release has been tested with real users, and the phone apps after that.
- The pixel-art office, which belongs to the desktop app.
- Businesses that do not build software, in the first launch. They come after it.
- AI providers and agent programs other than Claude, until after the first real-user test.
- Teams larger than seven, several people on one team, and running with no person at all.
- Large-company needs: single sign-on, security certifications and private AI model connections.
- Paid cloud hosting of customers' workspaces, and Slack, which come last.

## How we'll know it works

- **The harness test.** On Catervas's own project, the Product Manager writes contracts for three real issues, one Developer builds them, a second checks them, the Product Manager accepts them, and the founder reviews the changes and the record and agrees each was done as agreed.
- **The thirty-minute test.** A new user with no help goes from nothing to an accepted task on their own project in under thirty minutes, measured with five people, at least three of them non-technical, in the browser.
- **The kit check.** Each role but the DevOps Engineer does one real task with its own tools in the web app, on Claude, recorded and signed off.
- **The benchmark.** Before launch, a published, blind comparison of a Catervas team against one fully equipped AI agent: Catervas must be at least as good on quality and clearly better on failures and cost control, or it does not launch and the product changes.
- **The numbers to watch:** tasks accepted on their first review; escalations per accepted task, which should fall over time; cost per accepted task and how much of it goes to planning and review; the share of checks run by a command or test rather than by opinion; and projects still active four weeks later.
- **Before the desktop app:** the founder and at least three non-technical testers run the released web app on their own projects, and the founder signs off the record.

## Notes on the sources

The project's front page still describes five roles with a Finance Specialist planned, while the specification lists more roles, the Finance Specialist, the UI/UX Designer and the Procurement Specialist among them. This document follows the specification. The early product analysis puts solo builders first; the founder later put the non-technical founder first, and this document follows that decision.
