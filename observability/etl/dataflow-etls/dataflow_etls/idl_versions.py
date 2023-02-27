import glob
import os
from pathlib import Path
from typing import List, Literal, Tuple, Optional
from anchorpy import Program, Provider, Wallet
from anchorpy.utils.rpc import AsyncClient
from anchorpy_core.idl import Idl
from solders.pubkey import Pubkey

Cluster = Literal["devnet", "mainnet"]
IdlBoundary = tuple[int, int]
ProgramIdlBoundaries = dict[str, List[IdlBoundary]]
ClusterIdlBoundaries = dict[Cluster, ProgramIdlBoundaries]


class VersionedProgram(Program):
    version: int
    cluster: Cluster

    def __init__(self, cluster: Cluster, version: int, idl: Idl, program_id: Pubkey,
                 provider: Optional[Provider] = None):
        self.version = version
        self.cluster = cluster
        super(VersionedProgram, self).__init__(idl, program_id,
                                               provider or Provider(AsyncClient("http://localhost:8899"),
                                                                    Wallet.dummy()))


class VersionedIdl:
    VERSIONS: ClusterIdlBoundaries = {"devnet": {
        "A7vUDErNPCTt9qrB6SSM4F6GkxzUe9d8P3cXSmRg4eY4": [(196494976, 0), (196520454, 1), (197246719, 2), (197494521, 3)]
    }}

    @staticmethod
    def get_idl_for_slot(cluster: Cluster, program_id: str, slot: int) -> Tuple[Idl, int]:
        idl_boundaries = VersionedIdl.VERSIONS[cluster][program_id]

        idl_version = None
        for boundary_slot, version in idl_boundaries:
            # todo: returns latest for upgrade slot, can throw if tx executed in same slot, before upgrade
            if boundary_slot > slot:
                idl_version = version
                break

        idl_dir = os.path.join(os.path.dirname(os.path.realpath(__file__)), f"idls/{cluster}")
        if idl_version is None:
            sorted_idls = [int(os.path.basename(path).removesuffix(".json").removeprefix("marginfi-v")) for path in
                           glob.glob(f"{idl_dir}/marginfi-v*.json")]
            sorted_idls.sort()
            idl_version = sorted_idls[-1]

        path = Path(f"{idl_dir}/marginfi-v{idl_version}.json")
        raw = path.read_text()
        idl = Idl.from_json(raw)

        return idl, idl_version
