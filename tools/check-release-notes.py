#!/usr/bin/env python3
"""Check a release body against the tagged checkout's changelog section."""

import argparse
import difflib
import json
from pathlib import Path
import posixpath
import re
import sys
from urllib.parse import quote


def normalized(text):
    return "\n".join(line.rstrip() for line in text.replace("\r\n", "\n").replace("\r", "\n").strip().splitlines())


def changelog_section(changelog, tag):
    version = tag.removeprefix("v")
    headings = list(re.finditer(r"^##[ \t]+\[?(v?\d+\.\d+\.\d+(?:[-+][\w.-]+)?)\]?(?:[ \t]|$).*$", changelog, re.M))
    matches = [(index, heading) for index, heading in enumerate(headings)
               if heading.group(1).removeprefix("v") == version]
    if len(matches) != 1:
        raise ValueError(f"CHANGELOG.md must have exactly one section for {tag}")
    index, heading = matches[0]
    next_heading = re.search(r"^##(?:[ \t]|$)", changelog[heading.end():], re.M)
    end = heading.end() + next_heading.start() if next_heading else len(changelog)
    section = changelog[heading.end():end]
    # Reference-link definitions belong to the file, not the release prose.
    section = re.sub(r"^\[(?!\^)[^\]]+\]:\s+\S[^\n]*\n?", "", section, flags=re.M)
    if not normalized(section):
        raise ValueError(f"CHANGELOG.md section for {tag} has no notes")
    previous = headings[index + 1].group(1).removeprefix("v") if index + 1 < len(headings) else None
    return normalized(section), previous


def release_links(text, repo, tag):
    def link(match):
        target = match.group(2)
        if target.startswith(("#", "//")) or re.match(r"[a-zA-Z][\w+.-]*:", target):
            return match.group(0)
        path = posixpath.normpath(target.lstrip("/"))
        if path == ".." or path.startswith("../"):
            raise ValueError(f"Relative release link escapes the repository: {target}")
        return match.group(1) + f"https://github.com/{repo}/blob/{quote(tag, safe='')}/{path}" + match.group(3)

    def inline(segment):
        output = []
        at = 0
        for span in re.finditer(r"(?<!`)(`+)(?!`)([\s\S]*?)(?<!`)\1(?!`)", segment):
            output.append(re.sub(r"(\]\()([^\s()]+)(\))", link, segment[at:span.start()]))
            output.append(span.group(0))
            at = span.end()
        output.append(re.sub(r"(\]\()([^\s()]+)(\))", link, segment[at:]))
        return "".join(output)

    output = []
    ordinary = []
    fence = None
    for line in text.splitlines(keepends=True):
        opener = re.match(r"^[ \t]*(`{3,}|~{3,})(.*)$", line.rstrip("\n"))
        if fence is not None:
            output.append(line)
            if opener and opener.group(1)[0] == fence[0] and len(opener.group(1)) >= fence[1] and not opener.group(2).strip():
                fence = None
        elif opener:
            output.append(inline("".join(ordinary)))
            ordinary = []
            output.append(line)
            fence = (opener.group(1)[0], len(opener.group(1)))
        else:
            ordinary.append(line)
    output.append(inline("".join(ordinary)))
    return "".join(output)


def check_release(changelog, release, repo, tag):
    if not re.fullmatch(r"[\w.-]+/[\w.-]+", repo):
        raise ValueError("Expected an owner/repository slug")
    if not isinstance(release, dict) or release.get("tag_name") != tag:
        raise ValueError(f"No release for {tag}")
    body = release.get("body")
    if not isinstance(body, str) or not body.strip():
        raise ValueError(f"The release for {tag} has no notes")
    section, previous = changelog_section(changelog, tag)
    prefix = "v" if tag.startswith("v") else ""
    footer_url = (f"https://github.com/{repo}/compare/{prefix}{previous}...{tag}"
                  if previous else f"https://github.com/{repo}/releases/tag/{tag}")
    footer = re.search(r"(?:^|\n)\*\*Full Changelog\*\*:\s*(\S+)\s*$", normalized(body))
    if not footer or footer.group(1) != footer_url:
        raise ValueError(f"Release notes need the footer: **Full Changelog**: {footer_url}")
    actual = normalized(normalized(body)[:footer.start()])
    expected = normalized(release_links(section, repo, tag))
    if actual != expected:
        difference = "\n".join(difflib.unified_diff(expected.splitlines(), actual.splitlines(),
                                                   fromfile="tagged CHANGELOG.md", tofile="release body", lineterm=""))
        raise ValueError(f"Release notes differ from the {tag} changelog section:\n{difference}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--repo", required=True)
    parser.add_argument("--changelog", default="CHANGELOG.md")
    parser.add_argument("--release-json", default="-")
    args = parser.parse_args()
    try:
        payload = sys.stdin.buffer.read().decode("utf-8") if args.release_json == "-" else Path(args.release_json).read_text(encoding="utf-8")
        if not payload.strip():
            raise ValueError(f"No release for {args.tag}")
        check_release(Path(args.changelog).read_text(encoding="utf-8"), json.loads(payload), args.repo, args.tag)
    except (ValueError, OSError) as error:
        print(f"::error::{error}", file=sys.stderr)
        return 1
    print(f"Release notes match CHANGELOG.md for {args.tag}.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
