function manifest = capabilityManifest()
%CAPABILITYMANIFEST Return the capability manifest (native execution profiles only).

manifest = struct();
manifest.schema = 'nirs4all-core.capabilities.v1';
manifest.aggregate = 'nirs4all-core';
manifest.runtimeSurfaces = nirs4all.runtimeSurfaces();
manifest.executionProfiles = nirs4all.executionProfiles();
end
