# M4-O4b execution metadata seam

Before editing the kernel: a delivered captured callback uses a newly issued
operation/lease. Owners cannot authenticate its source operation from guest bytes,
and equality with the old source OperationBinding necessarily fails. The existing
kernel callback provenance is authenticated but only available via a host lease
lookup. Expose that immutable provenance on the live CallerView; detached clones
do not retain it. No routing, grant, execution or version-selection API changes.
Owner references still validate the actual pure capture, current stamps and grants.
