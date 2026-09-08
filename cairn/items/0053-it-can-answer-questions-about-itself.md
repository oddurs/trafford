---
id: 53
key: v0.7
title: It can answer questions about itself
type: milestone
status: done
depends_on:
- 47
created: 2026-09-08
updated: 2026-09-08
priority: p1
due: 2027-02-28
---

Obsidian asks you to build the structure by hand, and then browse it. Measured
against a real 148-note vault, half of that bargain was never kept and the
other half was kept better than anyone noticed.

**Kept:** the frontmatter. 92 notes carry `type/reference`, 62 `status/active`,
25 `priority/high`, and there are `topic/*` families underneath. Roughly 220
typed property values across 148 notes. This vault is not a pile of prose; it
is a database somebody has been maintaining by hand for months.

**Not kept:** the graph. 66 of the 148 notes contain no `[[link]]` at all, 28
are linked to by nothing, and 13 distinct link targets name notes that were
never written.

**And nothing can see any of it.** The one thing Obsidian offers for asking a
vault a question is dataview, and this vault uses it in exactly one note out of
148. The structure is there. The query surface is not.

So v0.7 does not add intelligence to a pile of files. It gives a program that
already parses all of this a way to be asked. The file format does not change —
every note stays something Obsidian and `git diff` can both read — because that
format is what lets this vault be edited from Obsidian, from an agent, and from
trafford on the same afternoon. What changes is the model on top of it: the
vault stops being a graph you browse and becomes something you can question.

Three things follow, in order: the properties become real (0055), a query layer
reads them (0056), and it lands somewhere you can act on (0057). Everything
after that is a consumer — tasks (0027), suggested links (0059), grounded
answers (0060), and what has drifted (0061).

### What this milestone deliberately does not do

**Time as an axis.** A previous draft of this plan proposed making history
first-class: what you thought in March, how a note evolved. The vault's git log
is 42 commits, all dated 2026-05-01. There is no history to travel through, and
building a time machine over a single day's import would be building for a
vault nobody has. Killed by measurement; see 0031.

**A new file format.** Considered and rejected in one line: the format costs
nothing to keep, and it is the reason three different programs can edit these
notes.
