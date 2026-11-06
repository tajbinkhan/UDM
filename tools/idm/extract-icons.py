"""Write IDM's icon groups out as .ico files, one per group, named by ID.

    python -I tools/idm/extract-icons.py <rea-resources.json> <IDMan.exe> <out-folder>

The JSON is REA's `inspect_pe_resources` listing of IDMan.exe, saved to a
file. Each .ico holds the group's 16 px image, or its smallest one. Open the
folder in Explorer, or render a contact sheet, to see which icon is which:
in IDM 6.43, 128 is the globe and 288 the green arrows of the tray icon for
hidden download windows.
"""

import json
import os
import struct
import sys


def main():
    listing = json.load(open(sys.argv[1], encoding="utf-8"))["normalized_result"]
    exe = open(sys.argv[2], "rb").read()
    out = sys.argv[3]
    os.makedirs(out, exist_ok=True)
    resources = listing["resources"]
    for group in listing["icon_groups"]:
        res = resources[group["resource_index"]]
        gid = res["name"].get("id", res["name"].get("name"))
        images = [i for i in group["images"] if i.get("same_language_resource_index") is not None]
        if not images:
            continue
        small = [i for i in images if i["width"] == 16] or sorted(images, key=lambda i: i["width"] or 256)[:1]
        pick = sorted(small, key=lambda i: -i["bit_count"])[0]
        where = resources[pick["same_language_resource_index"]]["payload"]["location"]
        blob = exe[where["offset"] : where["offset"] + where["bytes"]]
        header = struct.pack("<HHH", 0, 1, 1)
        entry = struct.pack(
            "<BBBBHHII", pick["width"], pick["height"], pick["color_count"], 0, pick["planes"], pick["bit_count"], len(blob), 6 + 16
        )
        with open(os.path.join(out, "%s.ico" % gid), "wb") as f:
            f.write(header + entry + blob)
        print(gid, [(i["width"], i["bit_count"]) for i in group["images"]])


if __name__ == "__main__":
    main()
