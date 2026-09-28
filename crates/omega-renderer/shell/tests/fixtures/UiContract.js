.pragma library

// Generated from the shared event contracts and canonical protobuf JSON.
var nodes = ["stack","text","icon","header","separator","spacer","grid","button","slider","toggle","checkbox","form","field","textarea","list","group","dropdown","disclosure","dialog","progress","graph","image","viewport","badge","keycap","status","scroll"]
var cases = [
  {
    "absent": true,
    "encoded": null,
    "event": "press",
    "input": null,
    "kind": "button"
  },
  {
    "absent": false,
    "encoded": {
      "doubleValue": 1.0
    },
    "event": "change",
    "input": 1.0,
    "kind": "slider"
  },
  {
    "absent": false,
    "encoded": {
      "boolValue": true
    },
    "event": "change",
    "input": true,
    "kind": "toggle"
  },
  {
    "absent": false,
    "encoded": {
      "boolValue": true
    },
    "event": "change",
    "input": true,
    "kind": "checkbox"
  },
  {
    "absent": false,
    "encoded": {
      "map": {
        "entries": {
          "count": {
            "stringValue": "42"
          },
          "name": {
            "stringValue": "Ada"
          }
        }
      }
    },
    "event": "submit",
    "input": {
      "count": "42",
      "name": "Ada"
    },
    "kind": "form"
  },
  {
    "absent": false,
    "encoded": {
      "stringValue": "chosen"
    },
    "event": "submit",
    "input": "chosen",
    "kind": "field"
  },
  {
    "absent": false,
    "encoded": {
      "map": {
        "entries": {
          "reset": {
            "intValue": "4294967295"
          },
          "revision": {
            "intValue": "4294967295"
          },
          "text": {
            "stringValue": "chosen"
          }
        }
      }
    },
    "event": "change",
    "input": {
      "reset": 4294967295,
      "revision": 4294967295,
      "text": "chosen"
    },
    "kind": "field"
  },
  {
    "absent": false,
    "encoded": {
      "map": {
        "entries": {
          "reset": {
            "intValue": "4294967295"
          },
          "revision": {
            "intValue": "4294967295"
          },
          "text": {
            "stringValue": "chosen"
          }
        }
      }
    },
    "event": "change",
    "input": {
      "reset": 4294967295,
      "revision": 4294967295,
      "text": "chosen"
    },
    "kind": "textarea"
  },
  {
    "absent": false,
    "encoded": {
      "stringValue": "chosen"
    },
    "event": "select",
    "input": "chosen",
    "kind": "list"
  },
  {
    "absent": false,
    "encoded": {
      "stringValue": "chosen"
    },
    "event": "activate",
    "input": "chosen",
    "kind": "list"
  },
  {
    "absent": false,
    "encoded": {
      "stringValue": "chosen"
    },
    "event": "select",
    "input": "chosen",
    "kind": "group"
  },
  {
    "absent": false,
    "encoded": {
      "stringValue": "chosen"
    },
    "event": "select",
    "input": "chosen",
    "kind": "dropdown"
  },
  {
    "absent": false,
    "encoded": {
      "boolValue": true
    },
    "event": "toggle",
    "input": true,
    "kind": "disclosure"
  },
  {
    "absent": true,
    "encoded": null,
    "event": "confirm",
    "input": null,
    "kind": "dialog"
  },
  {
    "absent": true,
    "encoded": null,
    "event": "cancel",
    "input": null,
    "kind": "dialog"
  },
  {
    "absent": true,
    "encoded": null,
    "event": "dismiss",
    "input": null,
    "kind": "dialog"
  },
  {
    "absent": false,
    "encoded": {
      "doubleValue": 1.0
    },
    "event": "wheel",
    "input": 1.0,
    "kind": "image"
  },
  {
    "absent": false,
    "encoded": {
      "map": {
        "entries": {
          "dx": {
            "doubleValue": 1.0
          },
          "dy": {
            "doubleValue": 1.0
          },
          "offset_x": {
            "doubleValue": 1.0
          },
          "offset_y": {
            "doubleValue": 1.0
          },
          "x": {
            "doubleValue": 1.0
          },
          "y": {
            "doubleValue": 1.0
          },
          "zoom": {
            "doubleValue": 1.0
          }
        }
      }
    },
    "event": "wheel",
    "input": {
      "dx": 1.0,
      "dy": 1.0,
      "offset_x": 1.0,
      "offset_y": 1.0,
      "x": 1.0,
      "y": 1.0,
      "zoom": 1.0
    },
    "kind": "viewport"
  },
  {
    "absent": false,
    "encoded": {
      "map": {
        "entries": {
          "dx": {
            "doubleValue": 1.0
          },
          "dy": {
            "doubleValue": 1.0
          },
          "offset_x": {
            "doubleValue": 1.0
          },
          "offset_y": {
            "doubleValue": 1.0
          },
          "x": {
            "doubleValue": 1.0
          },
          "y": {
            "doubleValue": 1.0
          },
          "zoom": {
            "doubleValue": 1.0
          }
        }
      }
    },
    "event": "drag",
    "input": {
      "dx": 1.0,
      "dy": 1.0,
      "offset_x": 1.0,
      "offset_y": 1.0,
      "x": 1.0,
      "y": 1.0,
      "zoom": 1.0
    },
    "kind": "viewport"
  },
  {
    "absent": false,
    "encoded": {
      "map": {
        "entries": {
          "dx": {
            "doubleValue": 1.0
          },
          "dy": {
            "doubleValue": 1.0
          },
          "offset_x": {
            "doubleValue": 1.0
          },
          "offset_y": {
            "doubleValue": 1.0
          },
          "x": {
            "doubleValue": 1.0
          },
          "y": {
            "doubleValue": 1.0
          },
          "zoom": {
            "doubleValue": 1.0
          }
        }
      }
    },
    "event": "pinch",
    "input": {
      "dx": 1.0,
      "dy": 1.0,
      "offset_x": 1.0,
      "offset_y": 1.0,
      "x": 1.0,
      "y": 1.0,
      "zoom": 1.0
    },
    "kind": "viewport"
  }
]
