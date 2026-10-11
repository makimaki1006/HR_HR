"""本番事故の型と Docker 入力再現の境界を、架空ファイルで検証する。"""
import tempfile
from pathlib import Path
import unittest

from check_docker_context import ContextError, materialize


class DockerContextTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.repo = Path(self.temp.name) / 'repo'
        self.root = Path(self.temp.name) / 'image'
        self.repo.mkdir()
        self.root.mkdir()

    def write(self, name, text):
        path = self.repo / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)

    def dockerfile(self, copies):
        self.write('Dockerfile', 'FROM node:22 AS web\nCOPY private.txt /app/\n'
                   'FROM rust:1.98 AS builder\nWORKDIR /app\n' + copies +
                   '\nFROM debian:bookworm\nCOPY private.txt /app/\n')

    def test_only_builder_and_copy_layouts(self):
        self.write('Cargo.toml', 'manifest')
        self.write('Cargo.lock', 'lock')
        self.write('src/main.rs', 'source')
        self.write('policy.json', 'policy')
        self.write('private.txt', 'outside builder')
        self.dockerfile('COPY Cargo.toml \\\n Cargo.lock ./\nCOPY src/ src/\n'
                        'COPY ["policy.json", "/app/frontend/policy.json"]')
        workdir = materialize(self.repo, self.root)
        self.assertEqual(workdir, self.root / 'app')
        self.assertEqual({p.relative_to(workdir).as_posix(): p.read_text()
                          for p in workdir.rglob('*') if p.is_file()},
                         {'Cargo.toml': 'manifest', 'Cargo.lock': 'lock',
                          'src/main.rs': 'source', 'frontend/policy.json': 'policy'})

    def test_single_file_copy_to_workdir_without_trailing_slash(self):
        self.write('Cargo.toml', 'manifest')
        for copy in ['COPY Cargo.toml .', 'COPY Cargo.toml ./',
                     'COPY Cargo.toml /app', 'COPY Cargo.toml /app/',
                     'COPY ["Cargo.toml", "."]']:
            with self.subTest(copy=copy), tempfile.TemporaryDirectory() as temp:
                self.dockerfile(copy)
                workdir = materialize(self.repo, Path(temp))
                self.assertTrue(workdir.is_dir())
                self.assertEqual((workdir / 'Cargo.toml').read_text(), 'manifest')

    def test_relative_workdir_copy_to_dot(self):
        self.write('policy.json', 'policy')
        self.dockerfile('WORKDIR nested\nCOPY policy.json .')
        workdir = materialize(self.repo, self.root)
        self.assertEqual(workdir, self.root / 'app/nested')
        self.assertEqual((workdir / 'policy.json').read_text(), 'policy')

    def test_all_workdirs_created_in_instruction_order(self):
        self.write('policy.json', 'policy')
        self.dockerfile('WORKDIR /earlier\nWORKDIR /later\n'
                        'COPY policy.json /earlier\nWORKDIR empty')
        workdir = materialize(self.repo, self.root)
        self.assertEqual((self.root / 'earlier/policy.json').read_text(), 'policy')
        self.assertEqual(workdir, self.root / 'later/empty')
        self.assertTrue(workdir.is_dir())
        self.assertEqual(list(workdir.iterdir()), [])

    def test_workdir_is_not_created_before_preceding_copy(self):
        self.write('policy.json', 'policy')
        self.dockerfile('COPY policy.json /later\nWORKDIR /later')
        with self.assertRaises(FileExistsError):
            materialize(self.repo, self.root)
        self.assertEqual((self.root / 'later').read_text(), 'policy')

    def test_ignore_patterns_negation_and_directory_contents(self):
        for name in ['src/main.rs', 'src/notes.md', 'src/nested/notes.md',
                     'src/node_modules/pkg/code.rs', 'src/keep.md']:
            self.write(name, name)
        self.write('.dockerignore', '**/node_modules/\nsrc/*.md\n!src/keep.md\n')
        self.dockerfile('COPY src/ src/')
        workdir = materialize(self.repo, self.root)
        self.assertEqual({p.relative_to(workdir).as_posix() for p in workdir.rglob('*') if p.is_file()},
                         {'src/main.rs', 'src/nested/notes.md', 'src/keep.md'})

    def test_reinclude_file_inside_excluded_directory(self):
        self.write('src/main.rs', 'source')
        self.write('src/private.rs', 'excluded')
        self.write('.dockerignore', 'src/\n!src/main.rs\n')
        self.dockerfile('COPY src/ src/')
        workdir = materialize(self.repo, self.root)
        self.assertEqual((workdir / 'src/main.rs').read_text(), 'source')
        self.assertFalse((workdir / 'src/private.rs').exists())

    def test_missing_copy_does_not_leak_from_checkout(self):
        self.write('src/main.rs', 'include_str!("../policy.json");')
        self.write('policy.json', 'policy')
        self.dockerfile('COPY src/ src/')
        workdir = materialize(self.repo, self.root)
        self.assertFalse((workdir / 'policy.json').exists())

    def test_explicitly_ignored_source_fails(self):
        self.write('policy.json', 'policy')
        self.write('.dockerignore', 'policy.json\n')
        self.dockerfile('COPY policy.json policy.json')
        with self.assertRaisesRegex(ContextError, '除外'):
            materialize(self.repo, self.root)

    def test_dockerfile_specific_ignore_takes_precedence(self):
        self.write('policy.json', 'policy')
        self.write('.dockerignore', 'policy.json\n')
        self.write('Dockerfile.dockerignore', '# no exclusions\n')
        self.dockerfile('COPY policy.json policy.json')
        self.assertEqual((materialize(self.repo, self.root) / 'policy.json').read_text(), 'policy')

    def test_unknown_syntax_fails_closed(self):
        for copy in ['COPY src/*.rs src/', 'COPY ${SOURCE} src/',
                     'COPY --from=web /output /app/', 'COPY --chown=root src/ src/',
                     'COPY ../secret /app/', 'ADD src/ src/']:
            with self.subTest(copy=copy):
                self.dockerfile(copy)
                with self.assertRaises(ContextError):
                    materialize(self.repo, self.root)

    def test_unknown_ignore_syntax_fails_closed(self):
        self.write('.dockerignore', '[ab].rs\n')
        self.dockerfile('COPY src/ src/')
        with self.assertRaisesRegex(ContextError, '未対応'):
            materialize(self.repo, self.root)

    def test_relative_workdir_and_single_file_directory_destination(self):
        self.write('policy.json', 'policy')
        self.dockerfile('WORKDIR nested\nCOPY policy.json config/')
        workdir = materialize(self.repo, self.root)
        self.assertEqual((workdir / 'config/policy.json').read_text(), 'policy')


if __name__ == '__main__':
    unittest.main()
