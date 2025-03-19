from google.protobuf.internal import containers as _containers
from google.protobuf.internal import enum_type_wrapper as _enum_type_wrapper
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from typing import ClassVar as _ClassVar, Iterable as _Iterable, Mapping as _Mapping, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class ConfigurationStatus(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    CONFIGURATION_STATUS_UNSPECIFIED: _ClassVar[ConfigurationStatus]
    CONFIGURATION_STATUS_NONE: _ClassVar[ConfigurationStatus]
    CONFIGURATION_STATUS_MISSING_REQUIRED_CONFIGURATION: _ClassVar[ConfigurationStatus]
    CONFIGURATION_STATUS_UNRECOGNIZED_CONFIGURATION: _ClassVar[ConfigurationStatus]
    CONFIGURATION_STATUS_INVALID_CONFIGURATION_VALUE: _ClassVar[ConfigurationStatus]
    CONFIGURATION_STATUS_INTERNAL_ERROR: _ClassVar[ConfigurationStatus]
    CONFIGURATION_STATUS_FILE_NOT_FOUND: _ClassVar[ConfigurationStatus]
    CONFIGURATION_STATUS_PARSE_ERROR: _ClassVar[ConfigurationStatus]
    CONFIGURATION_STATUS_ENV_VAR_NOT_SET: _ClassVar[ConfigurationStatus]
    CONFIGURATION_STATUS_MISSING_PROGRAM: _ClassVar[ConfigurationStatus]

class QueryState(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    QUERY_STATE_UNSPECIFIED: _ClassVar[QueryState]
    QUERY_STATE_SUBMIT_COMPLETE: _ClassVar[QueryState]
    QUERY_STATE_REPLY_IN_PROGRESS: _ClassVar[QueryState]
    QUERY_STATE_REPLY_COMPLETE: _ClassVar[QueryState]
    QUERY_STATE_SUBMIT_IN_PROGRESS: _ClassVar[QueryState]
CONFIGURATION_STATUS_UNSPECIFIED: ConfigurationStatus
CONFIGURATION_STATUS_NONE: ConfigurationStatus
CONFIGURATION_STATUS_MISSING_REQUIRED_CONFIGURATION: ConfigurationStatus
CONFIGURATION_STATUS_UNRECOGNIZED_CONFIGURATION: ConfigurationStatus
CONFIGURATION_STATUS_INVALID_CONFIGURATION_VALUE: ConfigurationStatus
CONFIGURATION_STATUS_INTERNAL_ERROR: ConfigurationStatus
CONFIGURATION_STATUS_FILE_NOT_FOUND: ConfigurationStatus
CONFIGURATION_STATUS_PARSE_ERROR: ConfigurationStatus
CONFIGURATION_STATUS_ENV_VAR_NOT_SET: ConfigurationStatus
CONFIGURATION_STATUS_MISSING_PROGRAM: ConfigurationStatus
QUERY_STATE_UNSPECIFIED: QueryState
QUERY_STATE_SUBMIT_COMPLETE: QueryState
QUERY_STATE_REPLY_IN_PROGRESS: QueryState
QUERY_STATE_REPLY_COMPLETE: QueryState
QUERY_STATE_SUBMIT_IN_PROGRESS: QueryState

class GetQuerySchemasRequest(_message.Message):
    __slots__ = ("empty",)
    EMPTY_FIELD_NUMBER: _ClassVar[int]
    empty: Empty
    def __init__(self, empty: _Optional[_Union[Empty, _Mapping]] = ...) -> None: ...

class GetQuerySchemasResponse(_message.Message):
    __slots__ = ("query_name", "key_schema", "output_schema")
    QUERY_NAME_FIELD_NUMBER: _ClassVar[int]
    KEY_SCHEMA_FIELD_NUMBER: _ClassVar[int]
    OUTPUT_SCHEMA_FIELD_NUMBER: _ClassVar[int]
    query_name: str
    key_schema: str
    output_schema: str
    def __init__(self, query_name: _Optional[str] = ..., key_schema: _Optional[str] = ..., output_schema: _Optional[str] = ...) -> None: ...

class SetConfigurationRequest(_message.Message):
    __slots__ = ("configuration",)
    CONFIGURATION_FIELD_NUMBER: _ClassVar[int]
    configuration: str
    def __init__(self, configuration: _Optional[str] = ...) -> None: ...

class SetConfigurationResponse(_message.Message):
    __slots__ = ("status", "message")
    STATUS_FIELD_NUMBER: _ClassVar[int]
    MESSAGE_FIELD_NUMBER: _ClassVar[int]
    status: ConfigurationStatus
    message: str
    def __init__(self, status: _Optional[_Union[ConfigurationStatus, str]] = ..., message: _Optional[str] = ...) -> None: ...

class GetDefaultPolicyExpressionRequest(_message.Message):
    __slots__ = ("empty",)
    EMPTY_FIELD_NUMBER: _ClassVar[int]
    empty: Empty
    def __init__(self, empty: _Optional[_Union[Empty, _Mapping]] = ...) -> None: ...

class GetDefaultPolicyExpressionResponse(_message.Message):
    __slots__ = ("policy_expression",)
    POLICY_EXPRESSION_FIELD_NUMBER: _ClassVar[int]
    policy_expression: str
    def __init__(self, policy_expression: _Optional[str] = ...) -> None: ...

class ExplainDefaultQueryRequest(_message.Message):
    __slots__ = ("empty",)
    EMPTY_FIELD_NUMBER: _ClassVar[int]
    empty: Empty
    def __init__(self, empty: _Optional[_Union[Empty, _Mapping]] = ...) -> None: ...

class ExplainDefaultQueryResponse(_message.Message):
    __slots__ = ("explanation",)
    EXPLANATION_FIELD_NUMBER: _ClassVar[int]
    explanation: str
    def __init__(self, explanation: _Optional[str] = ...) -> None: ...

class InitiateQueryProtocolRequest(_message.Message):
    __slots__ = ("query",)
    QUERY_FIELD_NUMBER: _ClassVar[int]
    query: Query
    def __init__(self, query: _Optional[_Union[Query, _Mapping]] = ...) -> None: ...

class InitiateQueryProtocolResponse(_message.Message):
    __slots__ = ("query",)
    QUERY_FIELD_NUMBER: _ClassVar[int]
    query: Query
    def __init__(self, query: _Optional[_Union[Query, _Mapping]] = ...) -> None: ...

class Query(_message.Message):
    __slots__ = ("id", "state", "publisher_name", "plugin_name", "query_name", "key", "output", "concern", "split")
    ID_FIELD_NUMBER: _ClassVar[int]
    STATE_FIELD_NUMBER: _ClassVar[int]
    PUBLISHER_NAME_FIELD_NUMBER: _ClassVar[int]
    PLUGIN_NAME_FIELD_NUMBER: _ClassVar[int]
    QUERY_NAME_FIELD_NUMBER: _ClassVar[int]
    KEY_FIELD_NUMBER: _ClassVar[int]
    OUTPUT_FIELD_NUMBER: _ClassVar[int]
    CONCERN_FIELD_NUMBER: _ClassVar[int]
    SPLIT_FIELD_NUMBER: _ClassVar[int]
    id: int
    state: QueryState
    publisher_name: str
    plugin_name: str
    query_name: str
    key: _containers.RepeatedScalarFieldContainer[str]
    output: _containers.RepeatedScalarFieldContainer[str]
    concern: _containers.RepeatedScalarFieldContainer[str]
    split: bool
    def __init__(self, id: _Optional[int] = ..., state: _Optional[_Union[QueryState, str]] = ..., publisher_name: _Optional[str] = ..., plugin_name: _Optional[str] = ..., query_name: _Optional[str] = ..., key: _Optional[_Iterable[str]] = ..., output: _Optional[_Iterable[str]] = ..., concern: _Optional[_Iterable[str]] = ..., split: bool = ...) -> None: ...

class Empty(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...
