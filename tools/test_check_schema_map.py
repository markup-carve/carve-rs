#!/usr/bin/env python3
"""The schema-map checker's divergence-reason gate, proven to fire.

    python3 -m unittest discover -s tools -p 'test_*.py'

`divergences_are_live` refuses a declaration whose DIFFERENCE disappears and
could not refuse one whose REASON went false while the difference persisted
(markup-carve/carve#2270). Four declarations stayed green through an upstream
decision saying "carve-grammars names no node for it yet".

So these run the four as they stood, against upstream as it now is, and require
the refusal. A gate nobody watched fire is the defect one layer up.

The upstream checkout is the real one - only the map varies - because the pin's
ancestry, the commit that touched the source and the published node list all
have to be real for the refusal to mean anything. `CARVE_GRAMMARS_DIR` names it;
the default matches the CI job's path.
"""

from __future__ import annotations

import importlib.util
import io
import json
import os
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from tempfile import TemporaryDirectory

HERE = Path(__file__).resolve().parent
MAP = HERE.parent / "resources" / "prosemirror-schema-map.json"
GRAMMARS = Path(os.environ.get("CARVE_GRAMMARS_DIR", HERE.parent / ".carve-grammars"))

spec = importlib.util.spec_from_file_location("check_schema_map", HERE / "check-schema-map.py")
check_schema_map = importlib.util.module_from_spec(spec)
spec.loader.exec_module(check_schema_map)

# The four whose reasons went false at carve-grammars 7b5771b4, with the nodes
# upstream publishes for them. Any one of them alone proves the gate; all four
# are here because all four were green.
WENT_FALSE = {
    "block_extension": "carveBlockExtension",
    "directive": "carveDirective",
    "ruby": "carveRuby",
    "small_caps": "carveSmallCaps",
}


def run(overrides=None):
    """Run the checker over the real map, with `overrides` merged into it.

    MERGED rather than substituted: the committed declarations stay, so the only
    thing that varies between two runs is the entry under test. Replacing the
    block instead leaves the real differences undeclared, and the run then fails
    on those instead of on the reason.
    """
    document = json.loads(MAP.read_text(encoding="utf-8"))
    document["_provenance"]["divergences"].update(overrides or {})
    with TemporaryDirectory() as tmp:
        path = Path(tmp) / "map.json"
        path.write_text(json.dumps(document), encoding="utf-8")
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            code = check_schema_map.main(["--grammars", str(GRAMMARS), "--map", str(path)])

    return code, out.getvalue() + err.getvalue()


@unittest.skipUnless(GRAMMARS.joinpath(".git").exists(), f"no carve-grammars checkout at {GRAMMARS}")
class DivergenceReasons(unittest.TestCase):
    def test_the_map_as_committed_passes(self):
        code, output = run()
        self.assertEqual(code, 0, output)
        self.assertIn("every assertion holds", output)

    def test_a_reason_upstream_has_caught_up_with_is_refused(self):
        entries = {
            ty: {"kind": "upstream-has-no-node", "node": node, "why": "this bridge builds no node for it yet"}
            for ty, node in WENT_FALSE.items()
        }
        code, output = run(entries)
        self.assertEqual(code, 1, output)
        self.assertIn("divergence_reasons_hold", output)
        for ty, node in WENT_FALSE.items():
            self.assertIn(f"{ty}: main publishes `{node}`", output)

    def test_the_same_evidence_passes_that_check_as_prose(self):
        """The control: the refusal above came from the reason, not the entries.

        Filed as `prose`, the four carry no claim about upstream, so the reason
        gate says nothing about them - and they fail the older check instead,
        having no difference left to explain.
        """
        entries = {ty: {"kind": "prose", "why": "no node built yet"} for ty in WENT_FALSE}
        code, output = run(entries)
        self.assertEqual(code, 1, output)
        self.assertNotIn("divergence_reasons_hold", output)
        # They fail the older check instead: four types with no difference left.
        self.assertIn("divergences_are_live", output)
        for ty in WENT_FALSE:
            self.assertIn(ty, output)

    def test_a_reason_claiming_a_node_upstream_dropped_is_refused(self):
        entries = {
            "figure_group": {
                "kind": "upstream-names-a-node",
                "node": "carveFigureGrouping",
                "why": "this bridge degrades it to the generic div mapping",
            },
        }
        code, output = run(entries)
        self.assertEqual(code, 1, output)
        self.assertIn("main publishes no `carveFigureGrouping`", output)

    def test_a_node_upstream_publishes_outside_types_resolves(self):
        """`carveUnsupported` belongs to no Carve type and is still a node."""
        published = check_schema_map.upstream_node_names(
            json.loads(
                check_schema_map.git(GRAMMARS, "show", "origin/main:tiptap/schema-map.json").stdout
                or check_schema_map.git(GRAMMARS, "show", "main:tiptap/schema-map.json").stdout
            )
        )
        self.assertIn("carveUnsupported", published)
        self.assertIn("carveEmptyMark", published)
        self.assertIn("carveAbbreviationDefinition", published)


class NamedNodeDecisions(unittest.TestCase):
    """`markCarrierNodes` and `preservationNodes` are decisions, not prose.

    `decisions()` read `types` and `unmapped` only, so a rename in either
    section passed the difference check in silence - measured against a
    carve-grammars branch renaming `carveEmptyMark`, where the checker printed
    "every assertion holds" and exited 0 (markup-carve/carve-rs#1971). The
    engine reads both sections: `src/prosemirror` picks the carrier by the entry
    whose `attrs` holds `markType`, and the preserved names by the entries that
    have an `attrs` at all.

    The mutants are applied to the DOCUMENT rather than to a git branch, and
    `compare` is the function the git plumbing feeds - the plumbing itself is
    covered by the class above.
    """

    def setUp(self):
        self.document = json.loads(MAP.read_text(encoding="utf-8"))

    def decisions_of(self, document):
        return check_schema_map.decisions(document)

    def mutate(self, before, after):
        text = json.dumps(self.document)
        self.assertIn(f'"{before}"', text, f"{before} is not in the map")

        return json.loads(text.replace(f'"{before}"', f'"{after}"'))

    def refusal(self, mutant):
        with self.assertRaises(check_schema_map.Failure) as caught:
            check_schema_map.compare(
                self.decisions_of(self.document),
                self.decisions_of(mutant),
                {},
                "pin_is_current",
                "the mutant",
            )
        self.assertEqual(caught.exception.check, "pin_is_current")

        return caught.exception.message

    def test_both_sections_reach_the_decision_set(self):
        keys = self.decisions_of(self.document)
        self.assertIn("carrier:carveEmptyMark", keys)
        self.assertIn("preserved:carveUnsupported", keys)
        self.assertIn("preserved:carveUnsupportedInline", keys)
        self.assertNotIn("carrier:about", keys, "the section's prose is not a node")
        self.assertNotIn("preserved:about", keys)

    def test_a_renamed_carrier_is_refused(self):
        message = self.refusal(self.mutate("carveEmptyMark", "carveEmptyMarkRENAMED"))
        self.assertIn("carrier:carveEmptyMark", message)
        self.assertIn("carrier:carveEmptyMarkRENAMED", message)

    def test_a_renamed_preserved_node_is_refused(self):
        message = self.refusal(
            self.mutate("carveUnsupportedInline", "carveUnsupportedInlineRENAMED")
        )
        self.assertIn("preserved:carveUnsupportedInline", message)

    def test_a_moved_carrier_attribute_is_refused(self):
        """The attribute the loader SELECTS BY, which a rename alone would keep."""
        message = self.refusal(self.mutate("markType", "markKind"))
        self.assertIn("carrier:carveEmptyMark", message)
        self.assertIn("markKind", message)

    def test_a_changed_group_is_refused(self):
        mutant = json.loads(json.dumps(self.document))
        mutant["markCarrierNodes"]["carveEmptyMark"]["group"] = "block"
        self.assertIn("carrier:carveEmptyMark", self.refusal(mutant))

    def test_the_map_against_itself_is_no_difference(self):
        """The control: the refusals above came from the mutation."""
        used = check_schema_map.compare(
            self.decisions_of(self.document),
            self.decisions_of(json.loads(MAP.read_text(encoding="utf-8"))),
            {},
            "pin_is_current",
            "itself",
        )
        self.assertEqual(used, set())

    def test_the_sections_are_named_where_the_loader_reads_them(self):
        """A rename of either section key would make the new decisions vanish."""
        self.assertEqual(
            [key for key, _ in check_schema_map.NAMED_NODE_SECTIONS],
            ["markCarrierNodes", "preservationNodes"],
        )
        for key, _ in check_schema_map.NAMED_NODE_SECTIONS:
            self.assertIsInstance(self.document.get(key), dict, f"{key} is not in the map")


class ReasonShapes(unittest.TestCase):
    """The shape gate needs no upstream: it reads the declaration alone."""

    def shape(self, entries):
        with self.assertRaises(check_schema_map.Failure) as caught:
            check_schema_map.reason_shapes(entries)
        self.assertEqual(caught.exception.check, "divergence_reasons_are_shaped")

        return caught.exception.message

    def test_a_bare_prose_string_is_refused(self):
        self.assertIn("not an object with a `kind`", self.shape({"ruby": "no node yet"}))

    def test_an_unknown_kind_is_refused(self):
        self.assertIn("`kind` is 'because'", self.shape({"ruby": {"kind": "because", "why": "x"}}))

    def test_a_claim_about_upstream_must_name_a_node(self):
        entries = {"ruby": {"kind": "upstream-has-no-node", "why": "x"}}
        self.assertIn("names no `node`", self.shape(entries))

    def test_a_node_name_upstream_could_not_publish_is_refused(self):
        entries = {"ruby": {"kind": "upstream-has-no-node", "node": "ruby", "why": "x"}}
        self.assertIn("is not a ProseMirror name upstream could publish", self.shape(entries))

    def test_every_kind_says_why(self):
        self.assertIn("says no `why`", self.shape({"ruby": {"kind": "prose"}}))

    def test_the_committed_map_is_shaped(self):
        document = json.loads(MAP.read_text(encoding="utf-8"))
        check_schema_map.reason_shapes(document["_provenance"]["divergences"])


if __name__ == "__main__":
    unittest.main()
