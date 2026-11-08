function profiles = executionProfiles()
%EXECUTIONPROFILES Return the versioned execution profiles and the R federation record.
%   The data is generated from compat/capabilities.toml (see
%   scripts/generate_capability_profiles.py) and embedded in this package, so
%   no sibling checkout is needed. Each call decodes a fresh copy.

profiles = jsondecode(nirs4all.executionProfilesJson());
end
