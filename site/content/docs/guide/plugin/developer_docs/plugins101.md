---
title: Plugins101
---
# Plugins 101
## The purpose of this document is to provide a brief overview of plugins. More detailed information can be found in the for-developers.md document.
## The author has attempted to synthezise the information about queries found from the developer and user documentation into a brief summary. This may be helpful to read before or after reading the more detailed documentation to better understand plugins and how the work. 
- Plugins allow users to query data sources
- Plugins can get information from other plugins
- Plugins must have a CLI that accepts a port argument. The port indicates where the plugin is listening. 
- Plugins can support multiple queries. 
- Query endpoints are the functions that implement those queries
- Top-level analysis plugins should have a default query endpoint that accepts a struct Target
- Policy expressions determine whether the analysis from the plugins passes or fails.
- Policy files are provided by the user. They specify which top-level plugins to execute and policy expressions
-  The struct Target encompasses all the relevant info of a repository that we want to analyze. The struct is generally a local repo or github URL or package id. 
- Every query endpoint must have a struct that implements the Query trait. You can also use the macro called 'query' to mark functions you've written as query endpoints 
- The Query trait has the input, output, and actual query logic. The actual query logic is contained in the run() function. The run() function takes a mutable plugin engine as a parameter
- A plugin engine is a struct that allows you to request information from other plugins.
- The input and output schema for the Query trait are in a json file format. Plugins return data/measurements on data as a json. 
- You can provide plugins with a set of parameters to configure the plugins behaviors
- In addition to query structs, plugins contain a Plugin trait:
- The functions/strings within the Plugin trait are:
    - Strings:
        - PUBLISHER: defines the publisher
        - NAME: defines the name
    - Functions:
        - set_config(): allows you to configure the plugin, parameter is a set of 'String' key-value pairs
        - queries(): binds all of your query structs (structs that implement the Query trait) to the plugin. Instantiates each struct and creates a NamedQuery instance with a name in the name field. The query name must be designated as a struct instead of a string.
        - explain_default_query(): explains the default query
        - default_polict_expression(): returns the default policy expression for your default query endpoint
- The PluginServer actually runs the plugin
- In short plugins contain query endpoints. Query endpoints must either A) implement the Query trait or B) mark a function as a query endpoint. In addition to query endpoints, plugins also contain a Plugin trait. Once you finish defining your plugin, you pass the plugin instance and port to the PluginServer.








