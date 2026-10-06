function text = nativeRecipeJSON(recipe)
%NATIVERECIPEJSON Marshal recipe arrays without collapsing singleton choices.
% Native Core validates fields, methods, roles and numerical parameters.
if ischar(recipe), text = recipe; return; end
if ~isstruct(recipe) || ~isscalar(recipe) || ~isfield(recipe, 'steps') || ~isfield(recipe, 'candidates')
    error('nirs4all:NativeRecipe', 'Recipe requires steps and candidates');
end
if ~iscell(recipe.steps)
    recipe.steps = arrayfun(@(step) step, recipe.steps, 'UniformOutput', false);
end
if ~iscell(recipe.candidates)
    recipe.candidates = arrayfun(@(choice) choice, recipe.candidates, 'UniformOutput', false);
end
text = jsonencode(recipe);
end
