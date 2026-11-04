# The main service of the Python test plugin, with the websockets package the bundled Python
# carries, as Ulanzi Studio's does.
import asyncio
import json
import sys

import websockets

PLUGIN = "com.taurilanzi.e2e.python"


async def main(address: str, port: str) -> None:
    async with websockets.connect(f"ws://{address}:{port}") as socket:
        await socket.send(json.dumps({"code": 0, "cmd": "connected", "uuid": PLUGIN}))
        version = ".".join(map(str, sys.version_info[:2]))
        await socket.send(json.dumps({"cmd": "logMessage", "uuid": PLUGIN, "key": "", "actionid": "", "message": f"python {version}", "level": "info"}))
        async for _ in socket:
            pass


asyncio.run(main(*sys.argv[1:3]))
