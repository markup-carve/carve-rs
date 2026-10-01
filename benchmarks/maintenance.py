import hashlib
import json
import subprocess
import sys
import time

results = []
for n in (128, 1024, 4096):
    for name in ('quoted_fences', 'verse_definitions', 'paragraphs'):
        if name == 'quoted_fences':
            source = '> ::: |\n> verse\n' + '> ```x\n' * n + '> :::\n'
        elif name == 'verse_definitions':
            source = '> ::: |\n' + '> [r]: /hidden\n' * n + '> :::\n\n[t][r]\n'
        else:
            source = 'plain paragraph\n\n' * n
        data = source.encode()

        def run():
            return subprocess.run([sys.argv[1], '--html'], input=data,
                                  stdout=subprocess.PIPE, check=True).stdout

        for _ in range(3):
            run()
        samples = []
        for _ in range(7):
            start = time.perf_counter_ns()
            output = run()
            samples.append((time.perf_counter_ns() - start) / 1e6)
        samples.sort()
        results.append(dict(name=name, n=n, bytes=len(data), median_ms=samples[3],
                            min_ms=samples[0], hash=hashlib.sha256(output).hexdigest()))
print(json.dumps(results, indent=2))
