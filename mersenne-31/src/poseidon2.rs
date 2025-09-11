//! Implementation of Poseidon2, see: `<https://eprint.iacr.org/2023/323>`
//!
//! For the diffusion matrix, 1 + Diag(V), we perform a search to find an optimized
//! vector V composed of elements with efficient multiplication algorithms in AVX2/AVX512/NEON.
//!
//! This leads to using small values (e.g. 1, 2) where multiplication is implemented using addition
//! and powers of 2 where multiplication is implemented using shifts.
//! Additionally, for technical reasons, having the first entry be -2 is useful.
//!
//! Optimized Diagonal for Mersenne31 width 16:
//! [-2, 2^0, 2, 4, 8, 16, 32, 64, 2^7, 2^8, 2^10, 2^12, 2^13, 2^14, 2^15, 2^16]
//! Optimized Diagonal for Mersenne31 width 24:
//! [-2, 2^0, 2, 4, 8, 16, 32, 64, 2^7, 2^8, 2^9, 2^10, 2^11, 2^12, 2^13, 2^14, 2^15, 2^16, 2^17, 2^18, 2^19, 2^20, 2^21, 2^22]
//! See poseidon2\src\diffusion.rs for information on how to double check these matrices in Sage.

use p3_field::PrimeCharacteristicRing;
use p3_poseidon2::{
    ExternalLayerConstants,
    ExternalLayer, GenericPoseidon2LinearLayers, InternalLayer, MDSMat4, Poseidon2,
    add_rc_and_sbox_generic, external_initial_permute_state, external_terminal_permute_state,
    internal_permute_state,
};

use crate::{
    Mersenne31, Poseidon2ExternalLayerMersenne31, Poseidon2InternalLayerMersenne31, from_u62,
};

/// Degree of the chosen permutation polynomial for Mersenne31, used as the Poseidon2 S-Box.
///
/// As p - 1 = 2×3^2×7×11×... the smallest choice for a degree D satisfying gcd(p - 1, D) = 1 is 5.
/// Currently pub(crate) as it is used in the default neon implementation. Once that is optimized
/// this should no longer be public.
pub(crate) const MERSENNE31_S_BOX_DEGREE: u64 = 5;

/// An implementation of the Poseidon2 hash function specialised to run on the current architecture.
///
/// It acts on arrays of the form either `[Mersenne31::Packing; WIDTH]` or `[Mersenne31; WIDTH]`. For speed purposes,
/// wherever possible, input arrays should of the form `[Mersenne31::Packing; WIDTH]`.
pub type Poseidon2Mersenne31<const WIDTH: usize> = Poseidon2<
    Mersenne31,
    Poseidon2ExternalLayerMersenne31<WIDTH>,
    Poseidon2InternalLayerMersenne31,
    WIDTH,
    MERSENNE31_S_BOX_DEGREE,
>;

/// Initial round constants for the 16-width Poseidon2 external layer on Mersenne-31.
///
/// Generated with https://github.com/SteinerMatthias/poseidon2/blob/main/poseidon2_rust_params.sage.
pub const MERSENNE31_RC16_EXTERNAL_INITIAL: [[Mersenne31; 16]; 4] = [
    Mersenne31::new_array([
        0x768bab52, 0x70e0ab7d, 0x3d266c8a, 0x6da42045, 0x600fef22, 0x41dace6b, 0x64f9bdd4,
        0x5d42d4fe, 0x76b1516d, 0x6fc9a717, 0x70ac4fb6, 0x00194ef6, 0x22b644e2, 0x1f7916d5,
        0x47581be2, 0x2710a123,
    ]),
    Mersenne31::new_array([
        0x6284e867, 0x018d3afe, 0x5df99ef3, 0x4c1e467b, 0x566f6abc, 0x2994e427, 0x538a6d42,
        0x5d7bf2cf, 0x7fda2dab, 0x0fd854c4, 0x46922fca, 0x3d7763a1, 0x19fd05ca, 0x0a4bbb43,
        0x15075851, 0x3d903d76,
    ]),
    Mersenne31::new_array([
        0x2d290ff7, 0x40809fa0, 0x59dac6ec, 0x127927a2, 0x6bbf0ea0, 0x0294140f, 0x24742976,
        0x6e84c081, 0x22484f4a, 0x354cae59, 0x0453ffe1, 0x3f47a3cc, 0x0088204e, 0x6066e109,
        0x3b7c4b80, 0x6b55665d,
    ]),
    Mersenne31::new_array([
        0x3bc4b897, 0x735bf378, 0x508daf42, 0x1884fc2b, 0x7214f24c, 0x7498be0a, 0x1a60e640,
        0x3303f928, 0x29b46376, 0x5c96bb68, 0x65d097a5, 0x1d358e9f, 0x4a9a9017, 0x4724cf76,
        0x347af70f, 0x1e77e59a,
    ]),
];

/// Final round constants for the 16-width Poseidon2 external layer on Mersenne-31.
///
/// Generated with https://github.com/SteinerMatthias/poseidon2/blob/main/poseidon2_rust_params.sage.
pub const MERSENNE31_RC16_EXTERNAL_FINAL: [[Mersenne31; 16]; 4] = [
    Mersenne31::new_array([
        0x57090613, 0x1fa42108, 0x17bbef50, 0x1ff7e11c, 0x047b24ca, 0x4e140275, 0x4fa086f5,
        0x079b309c, 0x1159bd47, 0x6d37e4e5, 0x075d8dce, 0x12121ca0, 0x7f6a7c40, 0x68e182ba,
        0x5493201b, 0x0444a80e,
    ]),
    Mersenne31::new_array([
        0x0064f4c6, 0x6467abe6, 0x66975762, 0x2af68f9b, 0x345b33be, 0x1b70d47f, 0x053db717,
        0x381189cb, 0x43b915f8, 0x20df3694, 0x0f459d26, 0x77a0e97b, 0x2f73e739, 0x1876c2f9,
        0x65a0e29a, 0x4cabefbe,
    ]),
    Mersenne31::new_array([
        0x5abd1268, 0x4d34a760, 0x12771799, 0x69a0c9ac, 0x39091e55, 0x7f611cd0, 0x3af055da,
        0x7ac0bbdf, 0x6e0f3a24, 0x41e3b6f7, 0x49b3756d, 0x568bc538, 0x20c079d8, 0x1701c72c,
        0x7670dc6c, 0x5a439035,
    ]),
    Mersenne31::new_array([
        0x7c93e00e, 0x561fbb4d, 0x1178907b, 0x02737406, 0x32fb24f1, 0x6323b60a, 0x6ab12418,
        0x42c99cea, 0x155a0b97, 0x53d1c6aa, 0x2bd20347, 0x279b3d73, 0x4f5f3c70, 0x0245af6c,
        0x238359d3, 0x49966a59,
    ]),
];

/// Round constants for the 16-width Poseidon2's internal layer on Mersenne-31.
///
/// Generated with https://github.com/SteinerMatthias/poseidon2/blob/main/poseidon2_rust_params.sage.
pub const MERSENNE31_RC16_INTERNAL: [Mersenne31; 13] = Mersenne31::new_array([
    0x7f7ec4bf, 0x0421926f, 0x5198e669, 0x34db3148, 0x4368bafd, 0x66685c7f, 0x78d3249a, 0x60187881,
    0x76dad67a, 0x0690b437, 0x1ea95311, 0x40e5369a, 0x38f103fc,
]);

/// A default Poseidon2 for BabyBear using the round constants from the Horizon Labs implementation.
pub fn default_mersenne31_poseidon2_16() -> Poseidon2Mersenne31<16> {
    Poseidon2::new(
        ExternalLayerConstants::new(
            MERSENNE31_RC16_EXTERNAL_INITIAL.to_vec(),
            MERSENNE31_RC16_EXTERNAL_FINAL.to_vec(),
    ),
    MERSENNE31_RC16_INTERNAL.to_vec(),
    )
}

/// Initial round constants for the 24-width Poseidon2 external layer on Mersenne-31.
///
/// Generated with https://github.com/SteinerMatthias/poseidon2/blob/main/poseidon2_rust_params.sage.
pub const MERSENNE31_RC24_EXTERNAL_INITIAL: [[Mersenne31; 24]; 4] = [
    Mersenne31::new_array([
        0x1feaba61, 0x53224454, 0x6bceb9e2, 0x5019f9b4, 0x48726592, 0x2b22d0a8, 0x6151bbf9,
        0x2f474b21, 0x2eb5f337, 0x3b645d87, 0x0942cef0, 0x65228c52, 0x78ffb30f, 0x4d2837c8,
        0x0e17ac4f, 0x05546686, 0x046c06cc, 0x0b51c3b6, 0x568db763, 0x38b334e4, 0x57f5acf0,
        0x19d32611, 0x77d02f4b, 0x6c82e9b8,
    ]),
    Mersenne31::new_array([
        0x7148c1b6, 0x08067c75, 0x46d1e8c9, 0x30973b07, 0x20614f3b, 0x5c3ff851, 0x30503329,
        0x4972e7cc, 0x02d1d8bc, 0x09d5bfa6, 0x097104c0, 0x7ba49a34, 0x4a07c2fc, 0x24c1ee69,
        0x28a6ab41, 0x5d9108a0, 0x3a7851c7, 0x1dd495f9, 0x12b49ff4, 0x7bad5760, 0x5fed64c2,
        0x66f5c96c, 0x7eafbd02, 0x39b3593b,
    ]),
    Mersenne31::new_array([
        0x4a653b49, 0x75091dc1, 0x56e488e0, 0x1704a355, 0x745e4ff3, 0x392ef16e, 0x31e33fdf,
        0x02c28c66, 0x36c3083a, 0x3104d1fa, 0x5b03cda3, 0x6641e1af, 0x37754b56, 0x396f5af9,
        0x1a1a461a, 0x688e26f2, 0x6f829784, 0x1bb91d69, 0x5b788016, 0x704aa5c5, 0x0181869c,
        0x41211e56, 0x0ce803a0, 0x23bff3a0,
    ]),
    Mersenne31::new_array([
        0x17fb7064, 0x47317220, 0x76914b53, 0x219c1905, 0x16655528, 0x4df35544, 0x60808465,
        0x3350f833, 0x03bccdc7, 0x0a87180a, 0x017a99f5, 0x6e945726, 0x15445504, 0x780533b1,
        0x3b91bf38, 0x3fc77eb1, 0x4b4d960e, 0x3cd93d2e, 0x0ea4e976, 0x1d5306cc, 0x3a7ac284,
        0x0ec22934, 0x4d979713, 0x51a41c65,
    ]),
];

/// Final round constants for the 24-width Poseidon2 external layer on Mersenne-31.
///
/// Generated with https://github.com/SteinerMatthias/poseidon2/blob/main/poseidon2_rust_params.sage.
pub const MERSENNE31_RC24_EXTERNAL_FINAL: [[Mersenne31; 24]; 4] = [
    Mersenne31::new_array([
        0x1c662299, 0x057c955a, 0x7ab6c0f2, 0x25a6ad0a, 0x75850b58, 0x48fd3793, 0x0b4366b1,
        0x0fdd0d49, 0x7db419f9, 0x49b9cc0f, 0x48949716, 0x29c35890, 0x76445485, 0x1c27d30c,
        0x10aa7a3b, 0x30f34fb6, 0x6fe06435, 0x02135ecd, 0x6caaba96, 0x3eb290d0, 0x22fd8d3b,
        0x768b1525, 0x5be95814, 0x523d7fe9,
    ]),
    Mersenne31::new_array([
        0x55e94cec, 0x47c42e1f, 0x1aa53b5e, 0x2fd1fe7e, 0x59230e91, 0x7472da66, 0x6443f2df,
        0x2d9de19d, 0x6f7f6a84, 0x77800430, 0x0f014bc8, 0x7bf3d095, 0x26afd318, 0x582561f7,
        0x5ee3198c, 0x6acc0000, 0x2f315e26, 0x27cac040, 0x2595081e, 0x5963b7da, 0x7e073565,
        0x6cf3f5f1, 0x09f8a3a4, 0x0da8ccfe,
    ]),
    Mersenne31::new_array([
        0x60be2365, 0x7ed742f5, 0x668b8031, 0x4bb03494, 0x59019333, 0x700e2878, 0x1cc45856,
        0x1d1617f7, 0x7b988da6, 0x4eb4936c, 0x78c9f87e, 0x63ce3e94, 0x7178341b, 0x45bc2f86,
        0x05b775bc, 0x704b0244, 0x29eed278, 0x47f43032, 0x2127b2e5, 0x1997903f, 0x24b3ce03,
        0x0c32298c, 0x7d2b6f3a, 0x17fcaa81,
    ]),
    Mersenne31::new_array([
        0x72f37fef, 0x3028e7a9, 0x5edd4d96, 0x1f96583b, 0x4cd6918a, 0x14880f0e, 0x69170359,
        0x173cbd33, 0x0969e7f4, 0x6e7f23ab, 0x6182ea87, 0x4dcb1f5c, 0x585fa113, 0x729cb3b6,
        0x01b3a27a, 0x1ba173e7, 0x4b33bcea, 0x63d93bbb, 0x6b3fbf99, 0x6f17e9d1, 0x0c3dd8ba,
        0x0bc1f9a8, 0x64d3f370, 0x465a6a18,
    ]),
];

/// Round constants for the 24-width Poseidon2's internal layer on Mersenne-31.
///
/// Generated with https://github.com/SteinerMatthias/poseidon2/blob/main/poseidon2_rust_params.sage.
pub const MERSENNE31_RC24_INTERNAL: [Mersenne31; 21] = Mersenne31::new_array([
    0x22776a11, 0x5fa34268, 0x1415528d, 0x563fbd14, 0x34f45244, 0x120ea1b6, 0x261368a5,
    0x27665ec1, 0x36be2805, 0x345c4784, 0x17efdcc1, 0x393e6530, 0x6da0b4b8, 0x31e5ded3,
    0x675b27ac, 0x0ae88c30, 0x577841cc, 0x5fe06dec, 0x56b0691a, 0x7242de1f, 0x3c377529,
]);

/// A default Poseidon2 for BabyBear using the round constants from the Horizon Labs implementation.
///
/// See https://github.com/HorizenLabs/poseidon2/blob/main/plain_implementations/src/poseidon2/poseidon2_instance_babybear.rs
pub fn default_mersenne31_poseidon2_24() -> Poseidon2Mersenne31<24> {
    Poseidon2::new(
        ExternalLayerConstants::new(
            MERSENNE31_RC24_EXTERNAL_INITIAL.to_vec(),
            MERSENNE31_RC24_EXTERNAL_FINAL.to_vec(),
    ),
    MERSENNE31_RC24_INTERNAL.to_vec(),
    )
}

/// An implementation of the matrix multiplications in the internal and external layers of Poseidon2.
///
/// This can act on `[A; WIDTH]` for any ring implementing `Algebra<Mersenne31>`.
/// If you have either `[Mersenne31::Packing; WIDTH]` or `[Mersenne31; WIDTH]` it will be much faster
/// to use `Poseidon2Mersenne31<WIDTH>` instead of building a Poseidon2 permutation using this.
pub struct GenericPoseidon2LinearLayersMersenne31 {}

const POSEIDON2_INTERNAL_MATRIX_DIAG_16_SHIFTS: [u8; 15] =
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 10, 12, 13, 14, 15, 16];

const POSEIDON2_INTERNAL_MATRIX_DIAG_24_SHIFTS: [u8; 23] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22,
];

/// Multiply state by the matrix (1 + Diag(V))
///
/// Here V is the vector [-2] + 1 << shifts. This used delayed reduction to be slightly faster.
fn permute_mut<const N: usize>(state: &mut [Mersenne31; N], shifts: &[u8]) {
    debug_assert_eq!(shifts.len() + 1, N);
    let part_sum: u64 = state[1..].iter().map(|x| x.value as u64).sum();
    let full_sum = part_sum + (state[0].value as u64);
    let s0 = part_sum + (-state[0]).value as u64;
    state[0] = from_u62(s0);
    for i in 1..N {
        let si = full_sum + ((state[i].value as u64) << shifts[i - 1]);
        state[i] = from_u62(si);
    }
}

impl InternalLayer<Mersenne31, 16, MERSENNE31_S_BOX_DEGREE> for Poseidon2InternalLayerMersenne31 {
    /// Perform the internal layers of the Poseidon2 permutation on the given state.
    fn permute_state(&self, state: &mut [Mersenne31; 16]) {
        internal_permute_state(
            state,
            |x| permute_mut(x, &POSEIDON2_INTERNAL_MATRIX_DIAG_16_SHIFTS),
            &self.internal_constants,
        )
    }
}

impl InternalLayer<Mersenne31, 24, MERSENNE31_S_BOX_DEGREE> for Poseidon2InternalLayerMersenne31 {
    /// Perform the internal layers of the Poseidon2 permutation on the given state.
    fn permute_state(&self, state: &mut [Mersenne31; 24]) {
        internal_permute_state(
            state,
            |x| permute_mut(x, &POSEIDON2_INTERNAL_MATRIX_DIAG_24_SHIFTS),
            &self.internal_constants,
        )
    }
}

impl<const WIDTH: usize> ExternalLayer<Mersenne31, WIDTH, MERSENNE31_S_BOX_DEGREE>
    for Poseidon2ExternalLayerMersenne31<WIDTH>
{
    /// Perform the initial external layers of the Poseidon2 permutation on the given state.
    fn permute_state_initial(&self, state: &mut [Mersenne31; WIDTH]) {
        external_initial_permute_state(
            state,
            self.external_constants.get_initial_constants(),
            add_rc_and_sbox_generic,
            &MDSMat4,
        );
    }

    /// Perform the terminal external layers of the Poseidon2 permutation on the given state.
    fn permute_state_terminal(&self, state: &mut [Mersenne31; WIDTH]) {
        external_terminal_permute_state(
            state,
            self.external_constants.get_terminal_constants(),
            add_rc_and_sbox_generic,
            &MDSMat4,
        );
    }
}

impl GenericPoseidon2LinearLayers<16> for GenericPoseidon2LinearLayersMersenne31 {
    fn internal_linear_layer<R: PrimeCharacteristicRing>(state: &mut [R; 16]) {
        let part_sum: R = state[1..].iter().cloned().sum();
        let full_sum = part_sum.clone() + state[0].clone();

        // The first three diagonal elements are -2, 1, 2 so we do something custom.
        state[0] = part_sum - state[0].clone();
        state[1] = full_sum.clone() + state[1].clone();
        state[2] = full_sum.clone() + state[2].double();

        // For the remaining elements we use the mul_2exp_u64 method.
        // We need state[1..] as POSEIDON2_INTERNAL_MATRIX_DIAG_16_SHIFTS
        // doesn't include the shift for the 0'th element as it is -2.
        state[1..]
            .iter_mut()
            .zip(POSEIDON2_INTERNAL_MATRIX_DIAG_16_SHIFTS)
            .skip(2)
            .for_each(|(val, diag_shift)| {
                *val = full_sum.clone() + val.clone().mul_2exp_u64(diag_shift as u64);
            });
    }
}

impl GenericPoseidon2LinearLayers<24> for GenericPoseidon2LinearLayersMersenne31 {
    fn internal_linear_layer<R: PrimeCharacteristicRing>(state: &mut [R; 24]) {
        let part_sum: R = state[1..].iter().cloned().sum();
        let full_sum = part_sum.clone() + state[0].clone();

        // The first three diagonal elements are -2, 1, 2 so we do something custom.
        state[0] = part_sum - state[0].clone();
        state[1] = full_sum.clone() + state[1].clone();
        state[2] = full_sum.clone() + state[2].double();

        // For the remaining elements we use the mul_2exp_u64 method.
        // We need state[1..] as POSEIDON2_INTERNAL_MATRIX_DIAG_24_SHIFTS
        // doesn't include the shift for the 0'th element as it is -2.
        state[1..]
            .iter_mut()
            .zip(POSEIDON2_INTERNAL_MATRIX_DIAG_24_SHIFTS)
            .skip(2)
            .for_each(|(val, diag_shift)| {
                *val = full_sum.clone() + val.clone().mul_2exp_u64(diag_shift as u64);
            });
    }
}

#[cfg(test)]
mod tests {
    use p3_symmetric::Permutation;
    use rand::SeedableRng;
    use rand_xoshiro::Xoroshiro128Plus;

    use super::*;

    type F = Mersenne31;

    // We need to make some round constants. We use Xoroshiro128Plus for this as we can easily match this PRNG in sage.
    // See: https://github.com/0xPolygonZero/hash-constants for the sage code used to create all these tests.

    /// Test on a roughly random input.
    /// This random input is generated by the following sage code:
    /// set_random_seed(16)
    /// vector([M31.random_element() for t in range(16)]).
    #[test]
    fn test_poseidon2_width_16_random() {
        let mut input: [F; 16] = Mersenne31::new_array([
            894848333, 1437655012, 1200606629, 1690012884, 71131202, 1749206695, 1717947831,
            120589055, 19776022, 42382981, 1831865506, 724844064, 171220207, 1299207443, 227047920,
            1783754913,
        ]);

        let expected: [F; 16] = Mersenne31::new_array([
            1124552602, 2127602268, 1834113265, 1207687593, 1891161485, 245915620, 981277919,
            627265710, 1534924153, 1580826924, 887997842, 1526280482, 547791593, 1028672510,
            1803086471, 323071277,
        ]);

        let mut rng = Xoroshiro128Plus::seed_from_u64(1);
        let perm = Poseidon2Mersenne31::new_from_rng_128(&mut rng);

        perm.permute_mut(&mut input);
        assert_eq!(input, expected);
    }

    /// Test on a roughly random input.
    /// This random input is generated by the following sage code:
    /// set_random_seed(24)
    /// vector([M31.random_element() for t in range(24)]).
    #[test]
    fn test_poseidon2_width_24_random() {
        let mut input: [F; 24] = Mersenne31::new_array([
            886409618, 1327899896, 1902407911, 591953491, 648428576, 1844789031, 1198336108,
            355597330, 1799586834, 59617783, 790334801, 1968791836, 559272107, 31054313,
            1042221543, 474748436, 135686258, 263665994, 1962340735, 1741539604, 2026927696,
            449439011, 1131357108, 50869465,
        ]);

        let expected: [F; 24] = Mersenne31::new_array([
            87189408, 212775836, 954807335, 1424761838, 1222521810, 1264950009, 1891204592,
            710452896, 957091834, 1776630156, 1091081383, 786687731, 1101902149, 1281649821,
            436070674, 313565599, 1961711763, 2002894460, 2040173120, 854107426, 25198245,
            1967213543, 604802266, 2086190331,
        ]);

        let mut rng = Xoroshiro128Plus::seed_from_u64(1);
        let perm = Poseidon2Mersenne31::new_from_rng_128(&mut rng);

        perm.permute_mut(&mut input);
        assert_eq!(input, expected);
    }
}
