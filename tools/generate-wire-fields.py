#!/usr/bin/env python3
"""Generate src/wire_fields.rs from the pinned tests/spec/resources/ast-schema.json.

PART 12 section 11 makes an ingest refuse a property the schema does not name,
which needs the named set AT RUNTIME - and the schema lives in the spec
submodule, which the published crate does not ship. So the map is generated and
committed, with a test that regenerates it and compares: one source of truth
(the schema), one artifact, and a diff the moment the two drift.

Writing the list by hand would be the schema expressed a second time in code,
and the failure mode of a stale copy is REFUSING VALID INPUT - which is why this
is a script rather than a table someone maintains.

    python3 tools/generate-wire-fields.py [--check]

Exit codes: 0 when the committed file matches, 1 when it does not (--check), 2
on a usage/setup error.
"""
from __future__ import annotations

import json
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
SCHEMA = ROOT / "tests/spec/resources/ast-schema.json"
TARGET = ROOT / "src/wire_fields.rs"

HELPERS = ("attrs", "pos")

# Engine features whose normative schema change is still on an unmerged spec
# branch. Keep the pinned corpus revision unchanged; once that pin contains a
# field, the set insertion below is simply a no-op.
PENDING_NODE_FIELDS = {"comment": ("delimited",)}


def render(schema: dict) -> str:
    defs = schema.get("$defs", {})
    by_type: dict[str, list[str]] = {}
    for definition in defs.values():
        properties = definition.get("properties")
        if not properties:
            continue
        type_const = properties.get("type", {}).get("const")
        if not isinstance(type_const, str):
            continue
        if definition.get("additionalProperties") is not False:
            raise SystemExit(f"{type_const} is not closed in the schema; section 11 has nothing to check against")
        by_type[type_const] = sorted(properties)

    for type_const, fields in PENDING_NODE_FIELDS.items():
        if type_const not in by_type:
            raise SystemExit(f"pending field names unknown node type {type_const}")
        by_type[type_const] = sorted(set(by_type[type_const]).union(fields))

    helpers: dict[str, list[str]] = {}
    for name in HELPERS:
        definition = defs.get(name, {})
        if definition.get("additionalProperties") is not False:
            raise SystemExit(f"{name} is not closed in the schema; section 11 has nothing to check against")
        helpers[name] = sorted(definition["properties"])

    # The same kind of record, but written INLINE on a node instead of pulled
    # from `$defs` - `table.rowGroups`. `HELPERS` names the shared ones and
    # cannot see these, so a key inside one rode straight in. Keyed by property
    # name, which is how the runtime check reaches an object-valued property.
    inline_objects: dict[str, list[str]] = {}
    for definition in defs.values():
        properties = definition.get("properties")
        if not properties:
            continue
        type_const = properties.get("type", {}).get("const")
        if not isinstance(type_const, str):
            continue
        for property_name, property_schema in sorted(properties.items()):
            if property_schema.get("type") != "object":
                continue
            own = property_schema.get("properties")
            if not own or property_schema.get("additionalProperties") is not False:
                continue
            inline_objects[property_name] = sorted(own)
    helpers.update(inline_objects)

    # A record the schema CLOSES but gives no `type` of its own, reached through
    # an array-valued property of a typed node - `citation_group.items`, whose
    # items are `citation` objects. The runtime check is keyed by a node's
    # `type`, so these are invisible to it and every field on one was accepted;
    # the position has to come with the field set, because the record carries
    # nothing that identifies it. HELPERS covers the object-valued ones (`attrs`,
    # `pos`), which the check reaches by property name instead.
    untyped_arrays: dict[str, list[str]] = {}
    for definition in defs.values():
        properties = definition.get("properties")
        if not properties:
            continue
        type_const = properties.get("type", {}).get("const")
        if not isinstance(type_const, str):
            continue
        for property_name, property_schema in sorted(properties.items()):
            if property_schema.get("type") != "array":
                continue
            ref = (property_schema.get("items") or {}).get("$ref")
            if not isinstance(ref, str) or not ref.startswith("#/$defs/"):
                continue
            target = defs.get(ref[len("#/$defs/") :], {})
            target_properties = target.get("properties")
            if not target_properties:
                continue
            if isinstance(target_properties.get("type", {}).get("const"), str):
                continue  # a typed node; WIRE_FIELDS already closes it
            if target.get("additionalProperties") is not False:
                continue  # open in the schema; section 11 has nothing to check
            key = f"{type_const}.{property_name}"
            untyped_arrays[key] = sorted(target_properties)

        # And the same, one level down inside an inline object: the body groups
        # of `table.rowGroups.bodies` are records the schema closes, reached
        # through neither a `$defs` ref nor a top-level property. The runtime
        # check walks the dotted path.
        for property_name, property_schema in sorted(properties.items()):
            if property_schema.get("type") != "object":
                continue
            for sub_name, sub_schema in sorted((property_schema.get("properties") or {}).items()):
                if sub_schema.get("type") != "array":
                    continue
                items = sub_schema.get("items") or {}
                item_properties = items.get("properties")
                if not item_properties or items.get("additionalProperties") is not False:
                    continue
                if isinstance(item_properties.get("type", {}).get("const"), str):
                    continue  # a typed node; WIRE_FIELDS already closes it
                untyped_arrays[f"{type_const}.{property_name}.{sub_name}"] = sorted(item_properties)

    def table(entries: dict[str, list[str]], name: str, doc: str) -> str:
        rows = "".join(
            '    ("%s", &[%s]),\n' % (key, ", ".join('"%s"' % f for f in fields))
            for key, fields in sorted(entries.items())
        )
        # `rustfmt::skip` because the generator, not rustfmt, owns this
        # layout: without it the two disagree and the drift check fails on a
        # file nobody edited.
        return (
            "/// %s\n"
            "#[rustfmt::skip]\n"
            "pub(crate) const %s: &[(&str, &[&str])] = &[\n%s];\n" % (doc, name, rows)
        )

    return (
        "// GENERATED by tools/generate-wire-fields.py from the pinned\n"
        "// tests/spec/resources/ast-schema.json. Do not edit: run the tool.\n"
        "//\n"
        "// PART 12 section 11 - a property the schema does not name is refused on\n"
        "// ingest - needs the named set at runtime, and the schema is not shipped\n"
        "// with the crate.\n"
        "\n"
        + table(by_type, "WIRE_FIELDS", "Properties the schema names for each node type.")
        + "\n"
        + table(
            helpers,
            "WIRE_HELPER_FIELDS",
            "Properties the schema names for the objects that hang off a node.",
        )
        + "\n"
        + table(
            untyped_arrays,
            "WIRE_UNTYPED_ARRAY_FIELDS",
            "Properties the schema names for an untyped record in an array,\n"
            "/// keyed by the `type.property` that holds it.",
        )
    )


def main() -> int:
    if not SCHEMA.exists():
        print(f"{SCHEMA} is missing; run `git submodule update --init`", file=sys.stderr)
        return 2
    generated = render(json.loads(SCHEMA.read_text()))
    if "--check" in sys.argv[1:]:
        current = TARGET.read_text() if TARGET.exists() else ""
        if current == generated:
            return 0
        print(f"{TARGET} is stale; run `python3 tools/generate-wire-fields.py`", file=sys.stderr)
        return 1
    TARGET.write_text(generated)
    print(f"{TARGET} written from {SCHEMA}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
