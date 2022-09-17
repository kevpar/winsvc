# Pre 1.0

* Graceful termination
* winsvc logging
* Config/check command
* Config file default is more readable
* Update existing service on register
* Service IO redirection
* Console creation
* Restart/recovery actions
* SD notify protocol
* Think through what can go wrong and how failures will look in logs

# Post 1.0

* Config relative to winsvc.exe path
* Service binary relative to config path
* PID file
* Diag tool
  * Inspect service runtime properties
  * SD notify protocol interactions
  * Trigger log rotation

# Other

* Check if service config path matches on service deletion
  Write reg key with config path on service registration
  On service deletion, check config path reg key
    Doesn't exist -> this service is not managed by wind
    Wrong value -> this service does not match the config provided