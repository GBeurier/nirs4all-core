function model = runPipeline(dataset, recipe, varargin)
%RUNPIPELINE Execute native catalog pipeline CV/OOF/selection/full refit.
model = nirs4all.NativePipeline.fit(recipe, dataset, varargin{:});
end
