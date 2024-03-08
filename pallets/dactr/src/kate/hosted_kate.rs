use super::{AppExtrinsic, AppId, BlockLength, Error, GDataProof, GProof, GRawScalar, GRow, Seed};
use avail_core::{BlockLengthColumns, BlockLengthRows};
use frame_system::header_builder::MIN_WIDTH;
#[cfg(feature = "std")]
use kate::{
	com::Cell,
	couscous::multiproof_params,
	gridgen::{AsBytes as _, EvaluationGrid as EGrid},
	pmp::m1_blst::M1NoPrecomp,
};

use sp_runtime::SaturatedConversion as _;
use sp_runtime_interface::runtime_interface;
use sp_std::vec::Vec;

#[cfg(feature = "std")]
static SRS: std::sync::OnceLock<M1NoPrecomp> = std::sync::OnceLock::new();

/// Hosted function to build the header using `kate` commitments.
#[runtime_interface]
pub trait HostedKate {
	fn grid(
		submitted: Vec<AppExtrinsic>,
		block_length: BlockLength,
		seed: Seed,
		selected_rows: Vec<u32>,
	) -> Result<Vec<GRow>, Error> {
		let (max_width, max_height) = to_width_height(&block_length);
		let selected_rows = selected_rows
			.into_iter()
			.map(usize::try_from)
			.collect::<Result<Vec<_>, _>>()?;

		let grid = EGrid::from_extrinsics(submitted, MIN_WIDTH, max_width, max_height, seed)?;
		let rows = selected_rows
			.into_iter()
			.map(|row_idx| {
				let row = grid.row(row_idx).ok_or(Error::MissingRow(row_idx as u32))?;
				row.iter()
					.map(|scalar| scalar.to_bytes().map(GRawScalar::from))
					.collect::<Result<Vec<_>, _>>()
					.map_err(|_| Error::InvalidScalarAtRow(row_idx as u32))
			})
			.collect::<Result<Vec<_>, _>>()?;

		Ok(rows)
	}

	fn app_data(
		submitted: Vec<AppExtrinsic>,
		block_length: BlockLength,
		seed: Seed,
		app_id: u32,
	) -> Result<Vec<Option<GRow>>, Error> {
		let (max_width, max_height) = to_width_height(&block_length);
		let grid = EGrid::from_extrinsics(submitted, MIN_WIDTH, max_width, max_height, seed)?;

		// let orig_dims = non_extended_dims(grid.dims()).ok_or(Error::InvalidDimension)?;
		let dims = grid.dims();
		let Some(rows) = grid.app_rows(AppId(app_id), Some(dims))? else {
			return Err(Error::AppRow);
		};

		let mut all_rows = vec![None; dims.height()];
		for (row_y, row) in rows {
			let g_row = row
				.into_iter()
				.map(|s| s.to_bytes().map(GRawScalar::from))
				.collect::<Result<Vec<_>, _>>()
				.map_err(|_| Error::InvalidScalarAtRow(row_y as u32))?;
			all_rows[row_y] = Some(g_row);
		}

		Ok(all_rows)
	}

	fn proof(
		extrinsics: Vec<AppExtrinsic>,
		block_len: BlockLength,
		seed: Seed,
		cells: Vec<(u32, u32)>,
	) -> Result<Vec<GDataProof>, Error> {
		let srs = SRS.get_or_init(multiproof_params);
		let (max_width, max_height) = to_width_height(&block_len);
		let grid = EGrid::from_extrinsics(extrinsics, MIN_WIDTH, max_width, max_height, seed)?;
		let poly = grid.make_polynomial_grid()?;

		let proofs = cells
			.into_iter()
			.map(|(row, col)| -> Result<GDataProof, Error> {
				let data: GRawScalar = grid
					.get(row as usize, col as usize)
					.ok_or(Error::MissingCell { row, col })?
					.to_bytes()
					.map(GRawScalar::from)
					.map_err(|_| Error::InvalidScalarAtRow(row))?;

				let cell = Cell::new(BlockLengthRows(row), BlockLengthColumns(col));
				let proof = poly
					.proof(srs, &cell)?
					.to_bytes()
					.map(GProof::from)
					.map_err(|_| Error::Proof)?;

				Ok((data, proof))
			})
			.collect::<Result<Vec<_>, _>>()?;

		Ok(proofs)
	}
}

fn to_width_height(block_len: &BlockLength) -> (usize, usize) {
	// even if we run on a u16 target this is fine
	let width = block_len.cols.0.saturated_into();
	let height = block_len.rows.0.saturated_into();
	(width, height)
}

/*
fn non_extended_dims(dims: Dimensions) -> Option<Dimensions> {
	// Dimension of no extended matrix.
	let rows = dims
		.rows()
		.get()
		.checked_div(NonZeroU16::get(ROW_EXTENSION))?;
	let cols = dims
		.cols()
		.get()
		.checked_div(NonZeroU16::get(COL_EXTENSION))?;

	Dimensions::new_from(rows, cols)
}*/