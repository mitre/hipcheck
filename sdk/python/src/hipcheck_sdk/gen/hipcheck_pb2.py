# SPDX-License-Identifier: Apache-2.0
# -*- coding: utf-8 -*-
# Generated by the protocol buffer compiler.  DO NOT EDIT!
# NO CHECKED-IN PROTOBUF GENCODE
# source: hipcheck.proto
# Protobuf Python Version: 5.29.0
"""Generated protocol buffer code."""
from google.protobuf import descriptor as _descriptor
from google.protobuf import descriptor_pool as _descriptor_pool
from google.protobuf import runtime_version as _runtime_version
from google.protobuf import symbol_database as _symbol_database
from google.protobuf.internal import builder as _builder
_runtime_version.ValidateProtobufRuntimeVersion(
    _runtime_version.Domain.PUBLIC,
    5,
    29,
    0,
    '',
    'hipcheck.proto'
)
# @@protoc_insertion_point(imports)

_sym_db = _symbol_database.Default()




DESCRIPTOR = _descriptor_pool.Default().AddSerializedFile(b'\n\x0ehipcheck.proto\x12\x0bhipcheck.v1\";\n\x16GetQuerySchemasRequest\x12!\n\x05\x65mpty\x18\x01 \x01(\x0b\x32\x12.hipcheck.v1.Empty\"X\n\x17GetQuerySchemasResponse\x12\x12\n\nquery_name\x18\x01 \x01(\t\x12\x12\n\nkey_schema\x18\x02 \x01(\t\x12\x15\n\routput_schema\x18\x03 \x01(\t\"0\n\x17SetConfigurationRequest\x12\x15\n\rconfiguration\x18\x01 \x01(\t\"]\n\x18SetConfigurationResponse\x12\x30\n\x06status\x18\x01 \x01(\x0e\x32 .hipcheck.v1.ConfigurationStatus\x12\x0f\n\x07message\x18\x02 \x01(\t\"F\n!GetDefaultPolicyExpressionRequest\x12!\n\x05\x65mpty\x18\x01 \x01(\x0b\x32\x12.hipcheck.v1.Empty\"?\n\"GetDefaultPolicyExpressionResponse\x12\x19\n\x11policy_expression\x18\x01 \x01(\t\"?\n\x1a\x45xplainDefaultQueryRequest\x12!\n\x05\x65mpty\x18\x01 \x01(\x0b\x32\x12.hipcheck.v1.Empty\"2\n\x1b\x45xplainDefaultQueryResponse\x12\x13\n\x0b\x65xplanation\x18\x01 \x01(\t\"A\n\x1cInitiateQueryProtocolRequest\x12!\n\x05query\x18\x01 \x01(\x0b\x32\x12.hipcheck.v1.Query\"B\n\x1dInitiateQueryProtocolResponse\x12!\n\x05query\x18\x01 \x01(\x0b\x32\x12.hipcheck.v1.Query\"\xb9\x01\n\x05Query\x12\n\n\x02id\x18\x01 \x01(\x05\x12&\n\x05state\x18\x02 \x01(\x0e\x32\x17.hipcheck.v1.QueryState\x12\x16\n\x0epublisher_name\x18\x03 \x01(\t\x12\x13\n\x0bplugin_name\x18\x04 \x01(\t\x12\x12\n\nquery_name\x18\x05 \x01(\t\x12\x0b\n\x03key\x18\x06 \x03(\t\x12\x0e\n\x06output\x18\x07 \x03(\t\x12\x0f\n\x07\x63oncern\x18\x08 \x03(\t\x12\r\n\x05split\x18\t \x01(\x08\"\x07\n\x05\x45mpty*\xca\x03\n\x13\x43onfigurationStatus\x12$\n CONFIGURATION_STATUS_UNSPECIFIED\x10\x00\x12\x1d\n\x19\x43ONFIGURATION_STATUS_NONE\x10\x01\x12\x37\n3CONFIGURATION_STATUS_MISSING_REQUIRED_CONFIGURATION\x10\x02\x12\x33\n/CONFIGURATION_STATUS_UNRECOGNIZED_CONFIGURATION\x10\x03\x12\x34\n0CONFIGURATION_STATUS_INVALID_CONFIGURATION_VALUE\x10\x04\x12\'\n#CONFIGURATION_STATUS_INTERNAL_ERROR\x10\x05\x12\'\n#CONFIGURATION_STATUS_FILE_NOT_FOUND\x10\x06\x12$\n CONFIGURATION_STATUS_PARSE_ERROR\x10\x07\x12(\n$CONFIGURATION_STATUS_ENV_VAR_NOT_SET\x10\x08\x12(\n$CONFIGURATION_STATUS_MISSING_PROGRAM\x10\t*\xb1\x01\n\nQueryState\x12\x1b\n\x17QUERY_STATE_UNSPECIFIED\x10\x00\x12\x1f\n\x1bQUERY_STATE_SUBMIT_COMPLETE\x10\x01\x12!\n\x1dQUERY_STATE_REPLY_IN_PROGRESS\x10\x02\x12\x1e\n\x1aQUERY_STATE_REPLY_COMPLETE\x10\x03\x12\"\n\x1eQUERY_STATE_SUBMIT_IN_PROGRESS\x10\x04\x32\xad\x04\n\rPluginService\x12^\n\x0fGetQuerySchemas\x12#.hipcheck.v1.GetQuerySchemasRequest\x1a$.hipcheck.v1.GetQuerySchemasResponse0\x01\x12_\n\x10SetConfiguration\x12$.hipcheck.v1.SetConfigurationRequest\x1a%.hipcheck.v1.SetConfigurationResponse\x12}\n\x1aGetDefaultPolicyExpression\x12..hipcheck.v1.GetDefaultPolicyExpressionRequest\x1a/.hipcheck.v1.GetDefaultPolicyExpressionResponse\x12h\n\x13\x45xplainDefaultQuery\x12\'.hipcheck.v1.ExplainDefaultQueryRequest\x1a(.hipcheck.v1.ExplainDefaultQueryResponse\x12r\n\x15InitiateQueryProtocol\x12).hipcheck.v1.InitiateQueryProtocolRequest\x1a*.hipcheck.v1.InitiateQueryProtocolResponse(\x01\x30\x01\x62\x06proto3')

_globals = globals()
_builder.BuildMessageAndEnumDescriptors(DESCRIPTOR, _globals)
_builder.BuildTopDescriptorsAndMessages(DESCRIPTOR, 'hipcheck_pb2', _globals)
if not _descriptor._USE_C_DESCRIPTORS:
  DESCRIPTOR._loaded_options = None
  _globals['_CONFIGURATIONSTATUS']._serialized_start=914
  _globals['_CONFIGURATIONSTATUS']._serialized_end=1372
  _globals['_QUERYSTATE']._serialized_start=1375
  _globals['_QUERYSTATE']._serialized_end=1552
  _globals['_GETQUERYSCHEMASREQUEST']._serialized_start=31
  _globals['_GETQUERYSCHEMASREQUEST']._serialized_end=90
  _globals['_GETQUERYSCHEMASRESPONSE']._serialized_start=92
  _globals['_GETQUERYSCHEMASRESPONSE']._serialized_end=180
  _globals['_SETCONFIGURATIONREQUEST']._serialized_start=182
  _globals['_SETCONFIGURATIONREQUEST']._serialized_end=230
  _globals['_SETCONFIGURATIONRESPONSE']._serialized_start=232
  _globals['_SETCONFIGURATIONRESPONSE']._serialized_end=325
  _globals['_GETDEFAULTPOLICYEXPRESSIONREQUEST']._serialized_start=327
  _globals['_GETDEFAULTPOLICYEXPRESSIONREQUEST']._serialized_end=397
  _globals['_GETDEFAULTPOLICYEXPRESSIONRESPONSE']._serialized_start=399
  _globals['_GETDEFAULTPOLICYEXPRESSIONRESPONSE']._serialized_end=462
  _globals['_EXPLAINDEFAULTQUERYREQUEST']._serialized_start=464
  _globals['_EXPLAINDEFAULTQUERYREQUEST']._serialized_end=527
  _globals['_EXPLAINDEFAULTQUERYRESPONSE']._serialized_start=529
  _globals['_EXPLAINDEFAULTQUERYRESPONSE']._serialized_end=579
  _globals['_INITIATEQUERYPROTOCOLREQUEST']._serialized_start=581
  _globals['_INITIATEQUERYPROTOCOLREQUEST']._serialized_end=646
  _globals['_INITIATEQUERYPROTOCOLRESPONSE']._serialized_start=648
  _globals['_INITIATEQUERYPROTOCOLRESPONSE']._serialized_end=714
  _globals['_QUERY']._serialized_start=717
  _globals['_QUERY']._serialized_end=902
  _globals['_EMPTY']._serialized_start=904
  _globals['_EMPTY']._serialized_end=911
  _globals['_PLUGINSERVICE']._serialized_start=1555
  _globals['_PLUGINSERVICE']._serialized_end=2112
# @@protoc_insertion_point(module_scope)
