function cli = nativeCli(cli)
%NATIVECLI Resolve the executable once so objects survive cwd/PATH changes.
if isempty(cli), cli = getenv('NIRS4ALL_CORE_CLI'); end
if isempty(cli), cli = 'nirs4all-core-archive'; end
cli = char(cli);
if ~isrow(cli) || any(cli == char(0)) || any(cli == newline)
    error('nirs4all:NativeCli', 'Invalid native CLI path');
end
if exist(cli, 'file') ~= 2
    entries = strsplit(getenv('PATH'), pathsep);
    names = {cli};
    if ispc, names{end+1} = [cli '.exe']; end
    found = '';
    for i = 1:numel(entries)
        for j = 1:numel(names)
            candidate = fullfile(entries{i}, names{j});
            if exist(candidate, 'file') == 2, found = candidate; break; end
        end
        if ~isempty(found), break; end
    end
    if isempty(found), error('nirs4all:NativeCli', 'Native CLI not found: %s', cli); end
    cli = found;
end
[ok, attributes] = fileattrib(cli);
if ~ok, error('nirs4all:NativeCli', 'Cannot resolve native CLI: %s', cli); end
cli = attributes.Name;
end
