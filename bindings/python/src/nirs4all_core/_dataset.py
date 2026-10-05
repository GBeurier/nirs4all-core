"""Thin optional IO facade; importing Core does not import dataset loaders."""
from __future__ import annotations

from typing import Any


def dataset(value: Any, **kwargs: Any) -> Any:
    """Construct a raw dataset through the optional IO package."""
    try:
        from nirs4all_io.public_dataset import dataset as construct
    except ImportError as error:
        raise ImportError("Public dataset construction requires nirs4all-io") from error
    return construct(value, **kwargs)


class Dataset:
    """Constructor facade returning the IO-owned dataset object."""
    def __new__(cls, value: Any, **kwargs: Any) -> Any:
        return dataset(value, **kwargs)

    @classmethod
    def from_sources(cls, sources: Any, **kwargs: Any) -> Any:
        return dataset(sources, **kwargs)

    @classmethod
    def from_dict(cls, value: Any) -> Any:
        return dataset(value)
