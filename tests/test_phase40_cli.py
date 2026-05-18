import argparse
import inspect
import unittest

import cli


def parser_commands(parser: argparse.ArgumentParser) -> set[str]:
    for action in parser._actions:
        if isinstance(action, argparse._SubParsersAction):
            return set(action.choices)
    return set()


class Phase40CliTests(unittest.TestCase):
    def test_cli_exposes_only_local_create_and_read_only_balance(self):
        commands = parser_commands(cli.build_parser())
        self.assertEqual(commands, {"create", "balance"})
        self.assertNotIn("transfer", commands)
        self.assertNotIn("stake", commands)
        self.assertNotIn("unstake", commands)
        self.assertNotIn("status", commands)

    def test_create_command_does_not_reprint_recovery_material_or_local_path(self):
        source = inspect.getsource(cli.cmd_create)
        self.assertNotIn("Mnemonic:", source)
        self.assertNotIn("Wallet saved to:", source)


if __name__ == "__main__":
    unittest.main()
