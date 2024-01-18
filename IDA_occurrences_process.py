#!/usr/bin/env python3

# Used to convert immediate value occurrences copied from IDA Pro to a more understandable representation
# Values needs to be searched (values of eMusicID enum):
# NONE, NUM, Result (Last Valid Music ID), Select (Play Correct Music), Menu (Play Correct Music),
# Tutorial (Last Game Music ID + 1, used for comparison sometimes), Last Game Music ID

import sys
import subprocess


class InstructionInfo:
    def __init__(self, addr: str, func: str, instruction: str):
        self.instruction = instruction

        self.addr = addr.split("71")[1]

        self.func = parse_func(func)

    def __str__(self) -> str:
        return "0x{}\t{}\t{}".format(self.addr, self.func, self.instruction)


def parse_func(func: str) -> str:
    if not func.startswith("sub_"):
        return func

    func = func.removeprefix("sub_710")
    search_string = "// RVA: 0x{}".format(func)

    before_lines = 5
    while True:
        proc = subprocess.Popen(
            ["rg", "-A1", "-B{}".format(before_lines), search_string, sys.argv[2]],
            stdout=subprocess.PIPE,
        )
        (out, _) = proc.communicate()
        out = str(out, "UTF-8")

        if out == "":
            return "sub_710{}".format(func)

        if not "class" in out:
            before_lines *= 2
        else:
            break

    lines = out.split("\n")
    lines = map(lambda s: s.strip(), lines)
    lines = filter(lambda s: s != "", lines)
    lines = list(lines)

    func_name = lines[-1]
    if "(" in func_name.split(" ")[2]:
        func_name = func_name.split(" ")[2].split("(")[0]
    else:
        func_name = func_name.split(" ")[3].split("(")[0]

    class_name = list(filter(lambda s: "class" in s, lines))
    class_names = class_name[-1]
    class_name = class_names.split(" ")[3]
    if class_name == ":" or class_name == "//":
        class_name = class_names.split(" ")[2]

    return "{}::{}".format(class_name, func_name)


BLACKLISTED_INFOS = [
    "BubbleGroup",
    "ChannelData",
    "ChannelServices",
    "EncodingTable",
    "ExecutionContext",
    "FileIOManager",
    "FilePanel",
    "InputManager",
    "NotificationDialogDetail",
    "PiaTestMenu",
    "PrivateGame",
    "PrivateHost",
    "PrivateMode",
    "PrivateRoom",
    "RandomMusicPanel",
    "RankedGameSettings",
    "RegexCharClass",
    "RemotingConfiguration",
    "RenderTexture",
    "RuntimeResource",
    "SaveData",
    "SceneArea",
    "SceneDebugTitle",
    "SceneResult",
    "ScoreEditorManager",
    "SemaphoreSlim",
    "SideStory",
    "SkeletonJson",
    "SkeletonRagdoll",
    "SoapServices",
    "StageData",
    "TMP_TextUtilities",
    "TerrainUtility",
    "TouchScreenState",
    "TypeDescriptor",
    "UguiNovelTextGenerator",
    "UserStatusManager",
    "UserStatusManager",
    "iTween",
    "sub_",
]

if __name__ == "__main__":
    if len(sys.argv) < 3:
        print("Usage: {} [INPUT_FILENAME] [DUMP_CS]".format(sys.argv[0]))
        exit(-1)

    with open(sys.argv[1], "r") as f:
        lines = f.readlines()
        lines = filter(lambda s: ".text.1" in s, lines)
        lines = filter(lambda s: "CMP " in s or "MOV " in s, lines)
        lines = map(lambda s: s.split("\t"), lines)
        infos = map(lambda ss: InstructionInfo(ss[0], ss[1], ss[2]), lines)
        infos = filter(lambda i: not any(map(lambda f: f in i.func, BLACKLISTED_INFOS)), infos)

        print("\n".join(map(str, infos)))
