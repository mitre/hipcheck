#!/bin/bash

source venv/bin/activate
python3 -m sdk.python.tests.example.example $*
