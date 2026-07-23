# MessageVersion

## Properties

Name | Type | Description | Notes
------------ | ------------- | ------------- | -------------
**serial** | **String** | Unique identifier of this message version. | 
**timestamp** | **i64** | Milliseconds since the Unix epoch at which this version was created. | 
**client_id** | Option<**String**> | Client ID of the user who performed the update or deletion. | [optional]
**description** | Option<**String**> | Optional description supplied with an update or deletion. | [optional]
**metadata** | Option<**std::collections::HashMap<String, String>**> | Optional metadata supplied with an update or deletion operation. | [optional]

[[Back to Model list]](../README.md#documentation-for-models) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to README]](../README.md)


