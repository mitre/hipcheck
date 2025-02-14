from dataclasses import dataclass

import hipcheck_sdk.gen as gen

class SdkError(Exception):
    pass

class ConfigError(Exception):
    pass

@dataclass
class InvalidConfigValue(ConfigError):
    field_name: str
    value: str
    reason: str

@dataclass
class MissingRequiredConfig(ConfigError):
    field_name: str
    field_type: str
    possible_values: list[str]

@dataclass
class UnrecognizedConfig(ConfigError):
    field_name: str
    field_value: str
    possible_confusables: list[str]

@dataclass
class UnspecifiedConfigError(ConfigError):
    message: str


def to_set_config_response(err: ConfigError) -> gen.SetConfigurationResponse:
    status = None
    message = ""

    if isinstance(err, InvalidConfigValue):
        status = gen.ConfigurationStatus.CONFIGURATION_STATUS_INVALID_CONFIGURATION_VALUE
        message = f"invalid value '{err.value}' for '{err.field_name}', reason: '{err.reason}'"

    elif isinstance(err, MissingRequiredConfig):
        status = gen.ConfigurationStatus.CONFIGURATION_STATUS_MISSING_REQUIRED_CONFIGURATION
        message = f"missing required config item '{err.field_name}' of type '{err.field_type}'"
        if err.possible_values.len() > 0:
            vals = ", ".join(err.possible_values)
            message += f"; possible values: {vals}"

    elif isinstance(err, UnrecognizedConfig):
        status = gen.ConfigurationStatus.CONFIGURATION_STATUS_UNRECOGNIZED_CONFIGURATION
        message = f"unrecognized field '{err.field_name}' with value '{field_value}'"
        if err.possible_confusables.len() > 0:
            vals = ", ".join(err.possible_confusables)
            message += f"; possible field names: {vals}"

    elif isinstance(err, UnspecifiedConfigError):
        status = gen.ConfigurationStatus.CONFIGURATION_STATUS_UNSPECIFIED
        message = error.message

    else:
        raise TypeError(f"Error - unrecognized ConfigError subclass {type(err)}")

    return gen.SetConfigurationResponse(status=status, message=message)
