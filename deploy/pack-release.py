"""把发布目录打成 zip 发行包。

Windows 下用 Compress-Archive 或 7z 打包时，中文文件名容易变成乱码
（Git Bash 传参给 bsdtar 会按 ANSI 编码解释）。用 Python 的 zipfile
写入可以正确设置 UTF-8 标志位，Windows 资源管理器解压不会乱码。

用法：
    python deploy/pack-release.py --version 0.1.0
    python deploy/pack-release.py --version 0.1.0 --src publish --out release
"""

import argparse
import os
import zipfile

# 发行包里要包含的条目（相对 --src 目录）
ITEMS = [
    "MonitorPortable",
    "monitor-shell",
    "monitor-history-viewer",
    "部署说明.txt",
]


def pack(src: str, out_file: str) -> None:
    if os.path.exists(out_file):
        os.remove(out_file)

    with zipfile.ZipFile(out_file, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as zf:
        for item in ITEMS:
            path = os.path.join(src, item)
            if not os.path.exists(path):
                print(f"[跳过] 不存在: {path}")
                continue
            if os.path.isfile(path):
                zf.write(path, item)
                continue
            for dirpath, _dirnames, filenames in os.walk(path):
                rel_dir = os.path.relpath(dirpath, src).replace("\\", "/")
                zf.write(dirpath, rel_dir + "/")  # 目录条目，保留空的 data/logs
                for name in filenames:
                    full = os.path.join(dirpath, name)
                    zf.write(full, os.path.relpath(full, src).replace("\\", "/"))

    size_mb = os.path.getsize(out_file) / 1024 / 1024
    print(f"打包完成: {out_file} ({size_mb:.2f} MB)")


def main() -> None:
    parser = argparse.ArgumentParser(description="打包 Windows 发行包")
    parser.add_argument("--version", default="0.1.0", help="版本号，用于文件名")
    parser.add_argument("--src", default="publish", help="发布目录")
    parser.add_argument("--out", default="release", help="输出目录")
    args = parser.parse_args()

    os.makedirs(args.out, exist_ok=True)
    out_file = os.path.join(args.out, f"Monitor-{args.version}-win-x64.zip")
    pack(args.src, out_file)


if __name__ == "__main__":
    main()
