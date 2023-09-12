#!/usr/bin/env python3

import sys


class IPConfig:
    def __init__(self, addr: str, func: str, instruction: str):
        self.addr = addr
        self.func = func
        self.instruction = instruction

    def to_toml(self) -> str:
        return "[[patches]]\n# {}\noffset = {}\ninstruction = \"{}\"\n".format(self.func, self.addr, self.instruction)


if __name__ == "__main__":
    if len(sys.argv) < 3:
        print("Usage: {} [OUT_TOML] [IDA_OCCURENCE_PROCESSED_TXT] ...")

    configs = []

    files = sys.argv[2:]
    for file in files:
        with open(file, 'r') as f:
            lines = f.readlines()
            lines = map(lambda l: l.strip(), lines)
            lines = filter(lambda l: l != "", lines)
            lines = list(filter(lambda l: not l.startswith('#'), lines))

            file_configs = map(lambda l: IPConfig(l.split('\t')[0], l.split('\t')[1], l.split('\t')[2]), lines)

            configs.extend(file_configs)

    with open(sys.argv[1], 'w') as f:
        toml_str = "\n".join(map(lambda c: c.to_toml(), configs))

        f.write(toml_str)
