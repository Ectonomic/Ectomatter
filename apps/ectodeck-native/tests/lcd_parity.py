#!/usr/bin/python3
"""The z10-lcd service's own frames, for Ectodeck's screen player to match.

Runs the service's code (z10lcd.py and z10anim.py from ~/dev/z10-lcd, or the
folder in Z10_LCD_DIR) on made-up inputs and writes what it sends to
fixtures/lcd_parity.json. src/lcd_parity.rs's test feeds Ectodeck's player
the same inputs and checks it sends the same frames, character for character
and at the same moments.

  1. The Stats page's rows, read from made-up computers.
  2. The service's main loop on a clock that only moves when it sleeps, with
     game-helper's note written at set moments: a launch and an exit, each
     with its steps, and a few awkward cases. Its random choices start from
     seeds 1, 2, 3 and on, one a sequence.

    python3 tests/lcd_parity.py
"""
import builtins, glob as globmod, json, os, random, shutil, sys, tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
SERVICE = os.environ.get('Z10_LCD_DIR', os.path.expanduser('~/dev/z10-lcd'))
sys.path.insert(0, SERVICE)
import z10anim  # noqa: E402
import z10lcd  # noqa: E402

# ------------------------------------------------------------------ 1. the Stats page

COMPUTERS = [
    # temp (m°C), each core's clock (kHz), /proc/stat's first line twice, MemTotal and MemAvailable (kB), the GPU
    dict(temp=64500, freqs=[4365000, 4400000, 4300000, 4410000], stat=[[100, 0, 50, 800, 50, 0, 0, 0], [180, 3, 90, 1500, 61, 0, 2, 0]],
         mem=[63119752, 36839852], gpu=[45, 12, 1837, 13.13, 31.84]),
    dict(temp=9000, freqs=[550000, 5875123], stat=[[1, 1, 1, 1, 1, 1, 1, 1], [1, 1, 1, 1, 1, 1, 1, 1]],
         mem=[16384000, 16000000], gpu=[101, 100, 210, 0.04999, 8.0]),
    dict(temp=105250, freqs=[3000000], stat=[[0, 0, 0, 0, 0, 0, 0, 0], [999, 0, 1, 0, 0, 0, 0, 0]],
         mem=[131072000, 1024], gpu=None),
    dict(temp=35499, freqs=[1999999, 2000001, 2000500], stat=[[5, 5, 5, 5, 5, 5, 5, 5], [5, 5, 5, 6, 5, 5, 5, 5]],
         mem=[8000000, 4000000], gpu=[0, 0, 0, 0.25, 0.75]),
]


def computer(root, c, stat):
    h = os.path.join(root, 'sys/class/hwmon/hwmon3')
    os.makedirs(h, exist_ok=True)
    open(os.path.join(h, 'name'), 'w').write('k10temp\n')
    open(os.path.join(h, 'temp1_input'), 'w').write(f"{c['temp']}\n")
    other = os.path.join(root, 'sys/class/hwmon/hwmon0')
    os.makedirs(other, exist_ok=True)
    open(os.path.join(other, 'name'), 'w').write('nvme\n')
    for i, f in enumerate(c['freqs']):
        d = os.path.join(root, f'sys/devices/system/cpu/cpu{i}/cpufreq')
        os.makedirs(d, exist_ok=True)
        open(os.path.join(d, 'scaling_cur_freq'), 'w').write(f'{f}\n')
    os.makedirs(os.path.join(root, 'sys/devices/system/cpu/cpufreq'), exist_ok=True)
    os.makedirs(os.path.join(root, 'proc'), exist_ok=True)
    open(os.path.join(root, 'proc/stat'), 'w').write('cpu  ' + ' '.join(map(str, stat)) + '\ncpu0 1 2 3 4\n')
    open(os.path.join(root, 'proc/meminfo'), 'w').write(f"MemTotal:       {c['mem'][0]} kB\nMemFree:        1 kB\nMemAvailable:   {c['mem'][1]} kB\n")


class FakeGpu:
    def __init__(self, r):
        self.r = r

    def read(self):
        if self.r is None:
            raise OSError('no GPU')
        t, u, clk, vu, vt = self.r
        return t, u, clk, vu, vt


def stats_cases():
    out = []
    real_open, real_glob = builtins.open, globmod.glob
    for c in COMPUTERS:
        root = tempfile.mkdtemp()
        computer(root, c, c['stat'][0])
        z10lcd.open = lambda p, *a, **k: real_open(root + p if p.startswith('/') else p, *a, **k)
        z10lcd.glob = type('G', (), {'glob': staticmethod(lambda pat: [p[len(root):] for p in real_glob(root + pat)])})
        z10lcd.K10 = '/sys/class/hwmon/hwmon3'
        z10lcd._prev = None
        z10lcd.cpu_load()  # primed, as main() primes it
        computer(root, c, c['stat'][1])
        rows = z10lcd.screen(FakeGpu(c['gpu']))
        out.append(dict(computer=c, rows=rows))
        shutil.rmtree(root)
    del z10lcd.open
    z10lcd.glob = globmod
    return out

# ------------------------------------------------------------------ 2. the loop

# (seconds, the note's fields, or None to remove it). "pid" "self" is the
# harness's own process, alive throughout; "dead" a process that isn't there.
SCENARIOS = {
    'launch and exit': [
        (3.0, dict(session='a1', pid='self', phase='launch', name='Pokémon™ Legends: Z-A', steps='Gamemode|CPU governor|HDR on', done='0', started='1000')),
        (4.2, dict(session='a1', pid='self', phase='launch', name='Pokémon™ Legends: Z-A', steps='Gamemode|CPU governor|HDR on', done='1', started='1000')),
        (7.9, dict(session='a1', pid='self', phase='launch', name='Pokémon™ Legends: Z-A', steps='Gamemode|CPU governor|HDR on', done='3', started='1000')),
        (30.0, dict(session='a1', pid='self', phase='exit', name='Pokémon™ Legends: Z-A', steps='HDR off|CPU governor', done='0', started='1000', ended='5583')),
        (31.5, dict(session='a1', pid='self', phase='exit', name='Pokémon™ Legends: Z-A', steps='HDR off|CPU governor', done='2', started='1000', ended='5583')),
        (33.0, dict(session='a1', pid='self', phase='exit', name='Pokémon™ Legends: Z-A', steps='HDR off|CPU governor', done='2', started='1000', ended='5583', finished='1')),
    ],
    'game-helper gone': [
        (1.5, dict(session='b2', pid='dead', phase='launch', name='Half-Life 2: Episode Two, Deluxe Edition', steps='One|Two', done='0')),
        (12.0, dict(session='b2', pid='dead', phase='exit', name='Half-Life 2', steps='', started='50', ended='80')),
    ],
    'short and odd': [
        (0.5, dict(session='c3', pid='self', phase='launch', name='', steps='', done='0')),
        (2.0, None),
        (9.0, dict(session='c3', pid='self', phase='launch', name='A', steps='', done='0')),
        (9.05, dict(session='c4', pid='self', phase='exit', name='Ünïcödé Ⅻ Game', steps='x', done='1', started='0', ended='59', finished='1')),
        (20.0, dict(session='c4', pid='self', phase='other', name='B')),
        (21.0, {}),
    ],
}


class Done(Exception):
    pass


def run_loop(events, until):
    d = tempfile.mkdtemp()
    note = os.path.join(d, 'session')
    clock = [0.0]
    pending = list(events)
    pushes, stats_n = [], [0]

    def fields(f):
        f = {k: (str(os.getpid()) if v == 'self' else '999999999' if v == 'dead' else v) for k, v in f.items()}
        return ''.join(f'{k}={v}\n' for k, v in f.items())

    def apply():
        while pending and pending[0][0] <= clock[0] + 1e-9:
            _, f = pending.pop(0)
            if f is None:
                if os.path.exists(note):
                    os.remove(note)
            else:
                tmp = note + '.tmp'
                open(tmp, 'w').write(fields(f))
                os.replace(tmp, note)

    def sleep(s):
        clock[0] += s
        apply()
        if clock[0] > until:
            raise Done()

    def stats(_gpu=None):
        stats_n[0] += 1
        n = stats_n[0]
        return [f'CPU {n % 100:>2}\xb0 {n % 7:>3}% {4000 + n:>4}MHz', f'GPU {n % 50:>2}\xb0', f'RAM {n}', 'VRAM' + str(n % 3)]

    seeds = [0]

    def seeded():
        seeds[0] += 1
        return random.Random(seeds[0])

    z10lcd.time = type('T', (), {'monotonic': staticmethod(lambda: clock[0]), 'sleep': staticmethod(sleep)})
    z10lcd.EVENT = note
    z10lcd.find_hidraw = lambda: '/dev/null'
    z10lcd.send = lambda dev, rows: pushes.append([round(clock[0], 6), list(rows)])
    z10lcd.screen = stats
    z10lcd.cpu_load = lambda: 0.0
    z10lcd.TRACE = None
    z10anim.random = type('R', (), {'Random': staticmethod(seeded)})
    apply()
    try:
        z10lcd.main()
    except Done:
        pass
    shutil.rmtree(d)
    return pushes


def main():
    out = dict(stats=stats_cases(), loops=[])
    for name, events in SCENARIOS.items():
        until = 45.0
        out['loops'].append(dict(name=name, until=until, events=[[t, f] for t, f in events], pushes=run_loop(events, until)))
    path = os.path.join(HERE, 'fixtures', 'lcd_parity.json')
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, 'w') as f:
        json.dump(out, f, ensure_ascii=False, indent=0)
    print(path, sum(len(l['pushes']) for l in out['loops']), 'frames')


if __name__ == '__main__':
    main()
