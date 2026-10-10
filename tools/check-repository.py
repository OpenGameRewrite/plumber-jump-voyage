"""Reject game data, keys and non-source files in the Git index."""
from pathlib import Path
import re
import subprocess

ROOT=Path(__file__).resolve().parents[1]
allowed={'.gitignore','Cargo.toml','Cargo.lock','rust-toolchain.toml','README.md','AGENTS.md','LICENSE'}
def main():
    paths=subprocess.check_output(['git','ls-files','-z'],cwd=ROOT).decode().split('\0')
    for name in filter(None,paths):
        p=Path(name)
        ok=name in allowed or p.parts[0]=='src' and p.suffix=='.rs' or p.parts[0]=='tools' and p.suffix in {'.py','.ps1','.cs'} or p.parts[0]=='docs' and p.suffix=='.md' or p.parts[:2]==('.github','workflows') and p.suffix=='.yml'
        if not ok:raise SystemExit('Non-source path in index: '+name)
        content=subprocess.check_output(['git','show',':'+name],cwd=ROOT).decode('utf-8')
        if '\0' in content:raise SystemExit('Binary file in index: '+name)
        if re.search(r'(?m)^\s*(?:master_key_[0-9a-f]{2}|header_key|titlekek_[0-9a-f]{2})\s*=\s*[0-9a-fA-F]{32,}',content):raise SystemExit('Key material in index: '+name)
        if re.search(r'\b(?:FUN|DAT)_[0-9a-fA-F]{8,}\b',content):raise SystemExit('Decompiler placeholder in index: '+name)
    print('Git index contains only allowed original text files.')
if __name__=='__main__':main()
