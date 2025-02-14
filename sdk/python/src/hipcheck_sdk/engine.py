from typing import Union, List

from hipcheck_sdk.error import SdkError

class PluginEngine:

    def __init__(self, session_id: int, tx, rx, drop_tx):
        self.id: int = session_id
        self.tx = tx    # @Todo - determine types for tx/rx/drop_tx
        self.rx = rx
        self.concerns: List[str] = []
        self.drop_tx
        self.mock_responses = {}  # @Todo - implement mock interface


    # @Todo - better target type hint / QueryTarget
    async def query(self, target: object, input: object) -> Union[object, SdkError]:
        raise NotImplementedError()


    # @Todo - complete class implementation
