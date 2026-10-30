//! Process loopback excludes Veyva without filtering or muting other audio.
use std::{sync::mpsc, time::Duration};
use windows::{
    core::{Interface, HRESULT, PROPVARIANT},
    Win32::{
        Foundation::{E_FAIL, E_POINTER},
        Media::Audio::*,
    },
};

#[windows::core::implement(IActivateAudioInterfaceCompletionHandler)]
struct Completion {
    ready: mpsc::SyncSender<()>,
    // The activation blob points into this stable allocation, including on timeout.
    _params: Box<AUDIOCLIENT_ACTIVATION_PARAMS>,
}
impl IActivateAudioInterfaceCompletionHandler_Impl for Completion_Impl {
    fn ActivateCompleted(
        &self,
        _: Option<&IActivateAudioInterfaceAsyncOperation>,
    ) -> windows::core::Result<()> {
        let _ = self.ready.try_send(());
        Ok(())
    }
}

/// # Safety
/// The caller must hold a live COM apartment for activation and client use.
pub unsafe fn activate() -> windows::core::Result<IAudioClient> {
    let mut params = Box::new(AUDIOCLIENT_ACTIVATION_PARAMS {
        ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
        Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
            ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                TargetProcessId: std::process::id(),
                ProcessLoopbackMode: PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE,
            },
        },
    });
    // Borrowed VT_BLOB: Completion owns the allocation, so PropVariantClear must not free it.
    let mut raw: windows_core::imp::PROPVARIANT = std::mem::zeroed();
    raw.Anonymous.Anonymous.vt = 65; // VT_BLOB
    raw.Anonymous.Anonymous.Anonymous.blob.cbSize =
        std::mem::size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32;
    raw.Anonymous.Anonymous.Anonymous.blob.pBlobData =
        (&mut *params as *mut AUDIOCLIENT_ACTIVATION_PARAMS).cast();
    let blob = std::mem::ManuallyDrop::new(PROPVARIANT::from_raw(raw));
    let (ready, receiver) = mpsc::sync_channel(1);
    let handler: IActivateAudioInterfaceCompletionHandler = Completion {
        ready,
        _params: params,
    }
    .into();
    let operation = ActivateAudioInterfaceAsync(
        VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
        &IAudioClient::IID,
        Some(&*blob),
        &handler,
    )?;
    receiver
        .recv_timeout(Duration::from_secs(2))
        .map_err(|_| windows::core::Error::new(E_FAIL, "Process audio activation timed out"))?;
    let mut result = HRESULT(0);
    let mut interface = None;
    operation.GetActivateResult(&mut result, &mut interface)?;
    result.ok()?;
    interface
        .ok_or_else(|| windows::core::Error::from(E_POINTER))?
        .cast()
}
