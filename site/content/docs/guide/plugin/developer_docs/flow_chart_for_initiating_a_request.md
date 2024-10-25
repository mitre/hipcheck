
---
title: Flowchart for requesting another plugin
---
```mermaid

   flowchart TB
      start--The starting_plugin's 
      query trait's run() function 
      takes in a plugin engine and 
      an input JSON value --> 
      starting_plugin--
      The plugin engine 
      exposes the function 
      query() which is defined by 
      SDK. SDK is a library crate
      with tools for simplifying 
      plugin development. 
      query() takes in the input 
      value from run() and the 
      target plugin/query 
      endpoint 
      as parameters--> 
      SDK--The query function 
      executes query_inner(). 
      query_inner() sends a gRPC message 
      from the plugin to the 
      hipcheck server to 
      request data from 
      the target 
      plugin/query endpoint --> 
      grpc--> hipcheck_server--query_inner() 
      returns the JSON result to 
      the outer query function-->SDK--The outer function 
      returns the JSON result 
      to the starting plugin-->starting_plugin
```