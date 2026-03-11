//! GPU encoder channel types for mpsc-based GPU compression.
//!
//! Provides the message types and channel handle for submitting
//! single mip-level images to a dedicated GPU worker thread.

#[cfg(feature = "gpu-encode")]
mod inner {
    use image::RgbaImage;
    use tokio::sync::{mpsc, oneshot};

    use crate::dds::{DdsError, DdsFormat};

    /// Maximum number of tiles to batch per GPU pass.
    pub const MAX_BATCH: usize = 4;

    /// Bounded channel capacity for GPU encode requests.
    pub const CHANNEL_CAPACITY: usize = 32;

    /// Message sent from callers to the GPU worker for compression.
    pub struct GpuEncodeRequest {
        /// A single mip-level RGBA image to compress.
        pub image: RgbaImage,
        /// The target block compression format.
        pub format: DdsFormat,
        /// One-shot channel for returning the compressed data (or error).
        pub response: oneshot::Sender<Result<Vec<u8>, DdsError>>,
    }

    /// Sender-side handle for submitting GPU encode requests.
    pub struct GpuEncoderChannel {
        sender: mpsc::Sender<GpuEncodeRequest>,
    }

    impl GpuEncoderChannel {
        /// Create a new channel handle wrapping the given sender.
        pub fn new(sender: mpsc::Sender<GpuEncodeRequest>) -> Self {
            Self { sender }
        }

        /// Returns `true` if the receiver end is still alive.
        pub fn is_connected(&self) -> bool {
            !self.sender.is_closed()
        }
    }
}

#[cfg(feature = "gpu-encode")]
pub use inner::*;

#[cfg(test)]
#[cfg(feature = "gpu-encode")]
mod tests {
    use super::*;
    use crate::dds::DdsFormat;
    use image::RgbaImage;
    use tokio::sync::{mpsc, oneshot};

    #[test]
    fn test_max_batch_constant() {
        assert_eq!(MAX_BATCH, 4);
    }

    #[test]
    fn test_channel_capacity_constant() {
        assert_eq!(CHANNEL_CAPACITY, 32);
    }

    #[test]
    fn test_gpu_encoder_channel_connected() {
        let (tx, _rx) = mpsc::channel::<GpuEncodeRequest>(CHANNEL_CAPACITY);
        let channel = GpuEncoderChannel::new(tx);
        assert!(channel.is_connected());
    }

    #[test]
    fn test_gpu_encoder_channel_disconnected_when_rx_dropped() {
        let (tx, rx) = mpsc::channel::<GpuEncodeRequest>(CHANNEL_CAPACITY);
        let channel = GpuEncoderChannel::new(tx);
        drop(rx);
        assert!(!channel.is_connected());
    }

    #[tokio::test]
    async fn test_gpu_encode_request_roundtrip() {
        let (tx, mut rx) = mpsc::channel::<GpuEncodeRequest>(CHANNEL_CAPACITY);
        let (resp_tx, resp_rx) = oneshot::channel();

        let image = RgbaImage::new(4, 4);
        let request = GpuEncodeRequest {
            image,
            format: DdsFormat::BC1,
            response: resp_tx,
        };

        tx.send(request).await.expect("send should succeed");

        let received = rx.recv().await.expect("should receive request");
        assert_eq!(received.format, DdsFormat::BC1);
        assert_eq!(received.image.width(), 4);
        assert_eq!(received.image.height(), 4);

        let mock_data = vec![0xDE, 0xAD];
        received
            .response
            .send(Ok(mock_data.clone()))
            .expect("response send should succeed");

        let result = resp_rx.await.expect("should receive response");
        assert_eq!(result.unwrap(), mock_data);
    }
}
