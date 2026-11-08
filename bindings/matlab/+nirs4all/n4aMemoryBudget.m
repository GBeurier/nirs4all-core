function text = n4aMemoryBudget(value)
%N4AMEMORYBUDGET Normalize an optional allocation budget to exact decimal text.
%   TEXT = nirs4all.n4aMemoryBudget(VALUE) returns '' when VALUE is [] or '' (no
%   --memory-budget-bytes flag is sent and the host adds no cap of its own),
%   otherwise the exact decimal digits of a budget in bytes. Accepted values are
%     * a nonnegative integer-valued double up to 9007199254740991 (2^53 - 1),
%     * a nonnegative integer-class scalar (uint64 keeps every bit), or
%     * decimal digits only (no sign or spaces; leading zeros are valid and kept
%       as given), whose value is at most 18446744073709551615 (uint64 maximum),
%       compared as text without any numeric conversion.
%   Anything else is refused: a larger double could already have been rounded,
%   so pass such budgets as text or uint64. Nothing is rounded or coerced, there
%   is no default and no memory detection; the native CLI owns the admission.
if isstring(value) && isscalar(value), value = char(value); end
if isempty(value) && (isnumeric(value) || ischar(value))
    text = ''; return;
end
if ischar(value)
    limit = '18446744073709551615';
    ok = isrow(value) && ~isempty(regexp(value, '^[0-9]+$', 'once'));
    if ok
        digits = regexprep(value, '^0+(?=.)', '');   % compare the value without leading zeros
        ok = numel(digits) <= numel(limit);
        if ok && numel(digits) == numel(limit)
            first = find(digits ~= limit, 1);   % equal-length digit strings compare lexicographically
            ok = isempty(first) || digits(first) < limit(first);
        end
    end
    if ~ok, refuse(); end
    text = value; return;
end
if ~(isnumeric(value) && isscalar(value) && isreal(value)), refuse(); end
if isfloat(value)
    if ~(isfinite(value) && value >= 0 && value == floor(value) && value <= 9007199254740991), refuse(); end
    value = uint64(value);
elseif value < 0
    refuse();
end
% Exact digit extraction in the integer class: no double is ever formed. `mod` is
% exact on integer types, and (value - digit) is an exact multiple of ten, so the
% integer division below is exact too. (Octave's idivide 'floor' goes through a
% double and rounds a uint64 above 2^53, so it is not used.)
ten = cast(10, class(value)); text = '';
if value == 0, text = '0'; end
while value > 0
    digit = mod(value, ten);
    text = [char(48 + double(digit)) text]; %#ok<AGROW>
    value = (value - digit) / ten;
end
end

function refuse()
error('nirs4all:N4aArgument', ['memoryBudgetBytes must be empty, a nonnegative integer up to 2^53-1, ' ...
    'a nonnegative integer-class scalar (uint64) or exact decimal text up to 18446744073709551615']);
end
