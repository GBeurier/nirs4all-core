function model = runMultimodal(dataset, recipe, sourcePolicies, varargin)
%RUNMULTIMODAL Native IO/Methods/DAG multimodal workflow with explicit policies.
model = nirs4all.NativeMultimodal.fit(recipe, sourcePolicies, dataset, varargin{:});
end
