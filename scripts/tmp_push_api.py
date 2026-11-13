#!/usr/bin/env python3
"""临时脚本：github.com 直连不通时，用 Git Data API 把本地 HEAD 推到 main。

改进版：本地拿不到远端提交对象时（fetch 也断），改成「拉远端整棵树 + 跟本地 ls-tree 对比」，
只给差异文件建 blob，再带 base_tree 提交。用完删除。
"""
import base64
import json
import subprocess
import sys
import urllib.request

REPO = "Jie-baobao/rclone-mount-manager"


def sh(*args):
    return subprocess.run(args, capture_output=True, check=True).stdout


def token():
    p = subprocess.run(
        ["git", "credential", "fill"],
        input=b"protocol=https\nhost=github.com\n\n",
        capture_output=True,
    ).stdout.decode()
    for line in p.splitlines():
        if line.startswith("password="):
            return line[len("password="):]
    sys.exit("no token")


TOKEN = token()


def api(method, path, payload=None):
    data = json.dumps(payload).encode() if payload is not None else None
    req = urllib.request.Request(
        f"https://api.github.com/repos/{REPO}{path}",
        data=data,
        method=method,
        headers={
            "Authorization": f"Bearer {TOKEN}",
            "Accept": "application/vnd.github+json",
            "Content-Type": "application/json",
            "User-Agent": "rmm-push",
        },
    )
    with urllib.request.urlopen(req) as r:
        return json.load(r)


head = sh("git", "rev-parse", "HEAD").decode().strip()
parent = api("GET", "/git/ref/heads/main")["object"]["sha"]
parent_tree = api("GET", f"/git/commits/{parent}")["tree"]["sha"]
print("parent", parent[:7], "tree", parent_tree[:7])

remote = {}
sha = parent_tree
for e in api("GET", f"/git/trees/{sha}?recursive=1")["tree"]:
    if e["type"] == "blob":
        remote[e["path"]] = e["sha"]

local = {}
for line in sh("git", "ls-tree", "-r", "HEAD").decode().splitlines():
    meta, path = line.split("\t", 1)
    mode, _typ, blob = meta.split()
    local[path] = blob

changed = [p for p in local if remote.get(p) != local[p]]
deleted = [p for p in remote if p not in local]
print(f"changed {len(changed)} / deleted {len(deleted)}")

entries = []
for path in changed:
    blob = sh("git", "cat-file", "blob", f"HEAD:{path}")
    res = api("POST", "/git/blobs", {
        "content": base64.b64encode(blob).decode(),
        "encoding": "base64",
    })
    entries.append({"path": path, "mode": "100644", "type": "blob", "sha": res["sha"]})
for path in deleted:
    entries.append({"path": path, "mode": "100644", "type": "blob", "sha": None})

tree = api("POST", "/git/trees", {"base_tree": parent_tree, "tree": entries})
msg = sh("git", "log", "-1", "--pretty=%B").decode().strip()
commit = api("POST", "/git/commits", {"message": msg, "tree": tree["sha"], "parents": [parent]})
api("PATCH", "/git/refs/heads/main", {"sha": commit["sha"], "force": False})
print("ref ->", commit["sha"], "| local head", head[:7])
