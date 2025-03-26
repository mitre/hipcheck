# SPDX-License-Identifier: Apache-2.0

from dataclasses import dataclass

import hipcheck_sdk.gen as gen


class SdkError(Exception):
    """Parent class for all errors defined and caught by the SDK"""

    pass


@dataclass
class UnspecifiedQueryState(SdkError):
    """Catchall error used when other `SdkError` variants don't apply"""

    pass


@dataclass
class UnexpectedRequestInProgress(SdkError):
    """Received a query with the unexpected state `RequestInProgress`"""

    pass


@dataclass
class UnexpectedReplyInProgress(SdkError):
    """Received a query with the unexpected state `ReplyInProgress`"""

    pass


@dataclass
class ReceivedReplyWhenExpectingSubmitChunk(SdkError):
    """Received a Reply-type query message when a Submit-type was expected"""

    pass


@dataclass
class ReceivedSubmitWhenExpectingReplyChunk(SdkError):
    """Received a Submit-type query message when a Reply-type was expected"""

    pass


@dataclass
class InvalidJsonInQueryKey(SdkError):
    """One of the key fields in the query object contained invalid JSON"""

    pass


@dataclass
class InvalidJsonInQueryOutput(SdkError):
    """One of the output fields in the query object contained invalid JSON"""

    pass


@dataclass
class MoreAfterQueryComplete(SdkError):
    """The session with the given id received more messages after a complete query was parsed"""

    id: int


@dataclass
class FailedToSendQueryFromSessionToServer(SdkError):
    """An error occurred while sending a query message to Hipcheck core"""

    pass


@dataclass
class UnknownPluginQuery(SdkError):
    """The target for a query was unrecognized"""

    pass


@dataclass
class InvalidQueryTargetFormat(SdkError):
    """The target string for a query was incorrectly formatted"""

    pass


class ConfigError(Exception):
    """Parent class for all errors defined and caught during `Plugin.set_config()`"""

    pass


@dataclass
class InvalidConfigValue(ConfigError):
    """The value `value` of config field `field_name` was invalid because of `reason`"""

    field_name: str
    value: str
    reason: str


@dataclass
class MissingRequiredConfig(ConfigError):
    """The config field `field_name` of type `field_type` was missing. Possible values include `possible_values`."""

    field_name: str
    field_type: str
    possible_values: list[str]


@dataclass
class UnrecognizedConfig(ConfigError):
    """The config field `field_name` with value `field_value` was unrecognized. Intended field name may have possibly
    been one of `possible_confusables`."""

    field_name: str
    field_value: str
    possible_confusables: list[str]


@dataclass
class UnspecifiedConfigError(ConfigError):
    """An unspecified error occurred during configuration"""

    message: str


def to_set_config_response(err: ConfigError) -> gen.SetConfigurationResponse:
    """
    Convert our `ConfigError` type to a string for use in the query protocol

    :param ConfigError err: The error instance to convert
    :return: An instance of the auto-generated SetConfigurationResponse type

    :meta private:
    """
    status = None
    message = ""

    if isinstance(err, InvalidConfigValue):
        status = (
            gen.ConfigurationStatus.CONFIGURATION_STATUS_INVALID_CONFIGURATION_VALUE
        )
        message = f"invalid value '{err.value}' for '{err.field_name}', reason: '{err.reason}'"

    elif isinstance(err, MissingRequiredConfig):
        status = (
            gen.ConfigurationStatus.CONFIGURATION_STATUS_MISSING_REQUIRED_CONFIGURATION
        )
        message = f"missing required config item '{err.field_name}' of type '{err.field_type}'"
        if err.possible_values.len() > 0:
            vals = ", ".join(err.possible_values)
            message += f"; possible values: {vals}"

    elif isinstance(err, UnrecognizedConfig):
        status = gen.ConfigurationStatus.CONFIGURATION_STATUS_UNRECOGNIZED_CONFIGURATION
        message = (
            f"unrecognized field '{err.field_name}' with value '{err.field_value}'"
        )
        if err.possible_confusables.len() > 0:
            vals = ", ".join(err.possible_confusables)
            message += f"; possible field names: {vals}"

    elif isinstance(err, UnspecifiedConfigError):
        status = gen.ConfigurationStatus.CONFIGURATION_STATUS_UNSPECIFIED
        message = err.message

    else:
        raise TypeError(f"Error - unrecognized ConfigError subclass {type(err)}")

    return gen.SetConfigurationResponse(status=status, message=message)
